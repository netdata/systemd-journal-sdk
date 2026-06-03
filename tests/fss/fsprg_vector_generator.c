/* SPDX-License-Identifier: LGPL-2.1-or-later */

// cppcheck-suppress-file missingIncludeSystem
#define _GNU_SOURCE

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

#include "fsprg.h"

#define SECPAR FSPRG_RECOMMENDED_SECPAR
#define SEEDLEN FSPRG_RECOMMENDED_SEEDLEN
#define KEYLEN 32

static void hex_encode(char *out, const uint8_t *in, size_t len) {
        static const char hex[] = "0123456789abcdef";
        for (size_t i = 0; i < len; i++) {
                out[i * 2 + 0] = hex[in[i] >> 4];
                out[i * 2 + 1] = hex[in[i] & 0x0f];
        }
        out[len * 2] = '\0';
}

static void print_escaped_json_string(FILE *out, const char *s) {
        fprintf(out, "\"");
        for (; *s; s++) {
                switch (*s) {
                case '"':  fprintf(out, "\\\""); break;
                case '\\': fprintf(out, "\\\\"); break;
                case '\b': fprintf(out, "\\b");  break;
                case '\f': fprintf(out, "\\f");  break;
                case '\n': fprintf(out, "\\n");  break;
                case '\r': fprintf(out, "\\r");  break;
                case '\t': fprintf(out, "\\t");  break;
                default:
                        if ((unsigned char)*s < 0x20)
                                fprintf(out, "\\u%04x", (unsigned char)*s);
                        else
                                fputc(*s, out);
                }
        }
        fprintf(out, "\"");
}

typedef struct FsprgBuffers {
        size_t msklen;
        size_t mpklen;
        size_t statelen;
        size_t hexbuf_len;
        uint8_t *msk;
        uint8_t *mpk;
        uint8_t *state0;
        uint8_t *key;
        uint8_t *evolved_state;
        uint8_t *seek_state;
        char *hex;
} FsprgBuffers;

static void init_buffer_lengths(FsprgBuffers *buffers) {
        size_t msklen = FSPRG_mskinbytes(SECPAR);
        size_t mpklen = FSPRG_mpkinbytes(SECPAR);
        size_t statelen = FSPRG_stateinbytes(SECPAR);
        size_t hexbuf_len = (msklen > mpklen ? msklen : mpklen);
        if (statelen > hexbuf_len)
                hexbuf_len = statelen;
        buffers->msklen = msklen;
        buffers->mpklen = mpklen;
        buffers->statelen = statelen;
        buffers->hexbuf_len = hexbuf_len * 2 + 1;
}

static int alloc_buffers(FsprgBuffers *buffers) {
        init_buffer_lengths(buffers);
        buffers->msk = malloc(buffers->msklen);
        buffers->mpk = malloc(buffers->mpklen);
        buffers->state0 = malloc(buffers->statelen);
        buffers->key = malloc(KEYLEN);
        buffers->evolved_state = malloc(buffers->statelen);
        buffers->seek_state = malloc(buffers->statelen);
        buffers->hex = malloc(buffers->hexbuf_len);
        return buffers->msk && buffers->mpk && buffers->state0 &&
               buffers->key && buffers->evolved_state &&
               buffers->seek_state && buffers->hex ? 0 : -ENOMEM;
}

static void free_buffers(FsprgBuffers *buffers) {
        free(buffers->msk);
        free(buffers->mpk);
        free(buffers->state0);
        free(buffers->key);
        free(buffers->evolved_state);
        free(buffers->seek_state);
        free(buffers->hex);
}

static int generate_seed_material(FsprgBuffers *buffers, const uint8_t *seed) {
        int r = FSPRG_GenMK(buffers->msk, buffers->mpk, seed, SEEDLEN, SECPAR);
        if (r < 0)
                return r;
        return FSPRG_GenState0(buffers->state0, buffers->mpk, seed, SEEDLEN);
}

static void print_seed_header(FILE *out, FsprgBuffers *buffers, const uint8_t *seed, const char *seed_desc) {
        fprintf(out, "    {\n");
        fprintf(out, "      \"seed_desc\": ");
        print_escaped_json_string(out, seed_desc);
        fprintf(out, ",\n");

        hex_encode(buffers->hex, seed, SEEDLEN);
        fprintf(out, "      \"seed_hex\": \"%s\",\n", buffers->hex);

        hex_encode(buffers->hex, buffers->msk, buffers->msklen);
        fprintf(out, "      \"msk_hex\": \"%s\",\n", buffers->hex);

        hex_encode(buffers->hex, buffers->mpk, buffers->mpklen);
        fprintf(out, "      \"mpk_hex\": \"%s\",\n", buffers->hex);

        hex_encode(buffers->hex, buffers->state0, buffers->statelen);
        fprintf(out, "      \"state0_hex\": \"%s\",\n", buffers->hex);
        fprintf(out, "      \"epochs\": [\n");
}

static int evolve_to_epoch(uint8_t *state, uint64_t epoch) {
        uint64_t current = FSPRG_GetEpoch(state);
        while (current < epoch) {
                int r = FSPRG_Evolve(state);
                if (r < 0)
                        return r;
                current = FSPRG_GetEpoch(state);
        }
        return 0;
}

static int derive_epoch_states(FsprgBuffers *buffers, const uint8_t *seed, uint64_t epoch) {
        memcpy(buffers->evolved_state, buffers->state0, buffers->statelen);
        memcpy(buffers->seek_state, buffers->state0, buffers->statelen);
        int r = evolve_to_epoch(buffers->evolved_state, epoch);
        if (r < 0)
                return r;
        r = FSPRG_Seek(buffers->seek_state, epoch, buffers->msk, seed, SEEDLEN);
        if (r < 0)
                return r;
        if (memcmp(buffers->evolved_state, buffers->seek_state, buffers->statelen) != 0) {
                fprintf(stderr, "FSPRG_Seek mismatch at epoch %llu\n", (unsigned long long) epoch);
                return -EIO;
        }
        return 0;
}

static int print_epoch_keys(FILE *out, FsprgBuffers *buffers) {
        fprintf(out, "          \"keys\": [\n");
        for (uint32_t idx = 0; idx <= 1; idx++) {
                int r = FSPRG_GetKey(buffers->evolved_state, buffers->key, KEYLEN, idx);
                if (r < 0)
                        return r;
                hex_encode(buffers->hex, buffers->key, KEYLEN);
                fprintf(out, "            {\"idx\": %u, \"keylen\": %u, \"key_hex\": \"%s\"}",
                        idx, (unsigned) KEYLEN, buffers->hex);
                if (idx < 1)
                        fprintf(out, ",");
                fprintf(out, "\n");
        }
        fprintf(out, "          ]\n");
        return 0;
}

static int print_epoch(FILE *out, FsprgBuffers *buffers, const uint8_t *seed, uint64_t epoch, int needs_comma) {
        int r = derive_epoch_states(buffers, seed, epoch);
        if (r < 0)
                return r;
        hex_encode(buffers->hex, buffers->evolved_state, buffers->statelen);
        fprintf(out, "        {\n");
        fprintf(out, "          \"epoch\": %llu,\n", (unsigned long long) epoch);
        fprintf(out, "          \"state_hex\": \"%s\",\n", buffers->hex);
        hex_encode(buffers->hex, buffers->seek_state, buffers->statelen);
        fprintf(out, "          \"seek_state_hex\": \"%s\",\n", buffers->hex);
        fprintf(out, "          \"seek_matches_evolved\": true,\n");
        r = print_epoch_keys(out, buffers);
        if (r < 0)
                return r;
        fprintf(out, "        }");
        if (needs_comma)
                fprintf(out, ",");
        fprintf(out, "\n");
        return 0;
}

static int print_epochs(FILE *out, FsprgBuffers *buffers, const uint8_t *seed) {
        uint64_t epochs[] = {0, 1, 2, 3, 17};
        size_t n_epochs = sizeof(epochs) / sizeof(epochs[0]);
        for (size_t i = 0; i < n_epochs; i++) {
                int r = print_epoch(out, buffers, seed, epochs[i], i + 1 < n_epochs);
                if (r < 0)
                        return r;
        }
        return 0;
}

static int run_seed(FILE *out, const uint8_t *seed, const char *seed_desc) {
        FsprgBuffers buffers = {0};
        int r = alloc_buffers(&buffers);
        if (r == 0)
                r = generate_seed_material(&buffers, seed);
        if (r == 0) {
                print_seed_header(out, &buffers, seed, seed_desc);
                r = print_epochs(out, &buffers, seed);
        }
        if (r == 0) {
                fprintf(out, "      ]\n");
                fprintf(out, "    }");
        }
        free_buffers(&buffers);
        return r;
}

static int generate_seed_json(char **json, size_t *json_len, const uint8_t *seed, const char *seed_desc) {
        FILE *out = open_memstream(json, json_len);
        if (!out)
                return errno ? -errno : -ENOMEM;

        int r = run_seed(out, seed, seed_desc);
        if (fclose(out) != 0 && r >= 0)
                r = errno ? -errno : -EIO;

        if (r < 0) {
                free(*json);
                *json = NULL;
                *json_len = 0;
        }

        return r;
}

int main(int argc, char **argv) {
        (void) argc;
        (void) argv;

        const uint8_t seed1[SEEDLEN] = {0};
        const uint8_t seed2[SEEDLEN] = {
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
                0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c
        };
        char *seed1_json = NULL;
        char *seed2_json = NULL;
        size_t seed1_json_len = 0;
        size_t seed2_json_len = 0;
        int r;

        r = generate_seed_json(&seed1_json, &seed1_json_len, seed1, "all-zeros");
        if (r < 0)
                goto fail;

        r = generate_seed_json(&seed2_json, &seed2_json_len, seed2, "incremental-0x01-0x0c");
        if (r < 0)
                goto fail;

        printf("{\n");
        printf("  \"vector_version\": \"1.0.0\",\n");
        printf("  \"systemd_baseline\": {\n");
        printf("    \"repo\": \"systemd/systemd\",\n");
        printf("    \"commit\": \"c0a5a2516d28601fb3afc1a77d7b42fcfe38fced\",\n");
        printf("    \"tag\": \"v260.1\"\n");
        printf("  },\n");
        printf("  \"fsprg_params\": {\n");
        printf("    \"secpar\": %u,\n", (unsigned) SECPAR);
        printf("    \"seedlen\": %zu,\n", (size_t) SEEDLEN);
        printf("    \"recommended_secpar\": true,\n");
        printf("    \"recommended_seedlen\": true,\n");
        printf("    \"keylen\": %u\n", (unsigned) KEYLEN);
        printf("  },\n");
        printf("  \"vectors\": [\n");
        printf("%s,\n", seed1_json);
        printf("%s\n", seed2_json);
        printf("  ]\n");
        printf("}\n");
        free(seed1_json);
        free(seed2_json);
        if (fflush(stdout) != 0) {
                fprintf(stderr, "failed to flush FSPRG vector JSON: %s\n", strerror(errno));
                fflush(stderr);
                _exit(EXIT_FAILURE);
        }
        _exit(EXIT_SUCCESS);

fail:
        free(seed1_json);
        free(seed2_json);
        fprintf(stderr, "fsprg vector generation failed: %d\n", r);
        fflush(stderr);
        _exit(EXIT_FAILURE);
}
