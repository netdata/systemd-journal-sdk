/* SPDX-License-Identifier: LGPL-2.1-or-later */

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

static int run_seed(FILE *out, const uint8_t *seed, const char *seed_desc) {
        size_t msklen = FSPRG_mskinbytes(SECPAR);
        size_t mpklen = FSPRG_mpkinbytes(SECPAR);
        size_t statelen = FSPRG_stateinbytes(SECPAR);
        size_t hexbuf_len = (msklen > mpklen ? msklen : mpklen);
        if (statelen > hexbuf_len)
                hexbuf_len = statelen;
        hexbuf_len = hexbuf_len * 2 + 1;

        uint8_t *msk = malloc(msklen);
        uint8_t *mpk = malloc(mpklen);
        uint8_t *state0 = malloc(statelen);
        uint8_t *key = malloc(KEYLEN);
        uint8_t *evolved_state = malloc(statelen);
        uint8_t *seek_state = malloc(statelen);
        char *hex = malloc(hexbuf_len);
        int r = 0;

        if (!msk || !mpk || !state0 || !key || !evolved_state || !seek_state || !hex) {
                r = -ENOMEM;
                goto finish;
        }

        r = FSPRG_GenMK(msk, mpk, seed, SEEDLEN, SECPAR);
        if (r < 0)
                goto finish;

        r = FSPRG_GenState0(state0, mpk, seed, SEEDLEN);
        if (r < 0)
                goto finish;

        fprintf(out, "    {\n");
        fprintf(out, "      \"seed_desc\": ");
        print_escaped_json_string(out, seed_desc);
        fprintf(out, ",\n");

        hex_encode(hex, seed, SEEDLEN);
        fprintf(out, "      \"seed_hex\": \"%s\",\n", hex);

        hex_encode(hex, msk, msklen);
        fprintf(out, "      \"msk_hex\": \"%s\",\n", hex);

        hex_encode(hex, mpk, mpklen);
        fprintf(out, "      \"mpk_hex\": \"%s\",\n", hex);

        hex_encode(hex, state0, statelen);
        fprintf(out, "      \"state0_hex\": \"%s\",\n", hex);

        fprintf(out, "      \"epochs\": [\n");

        uint64_t epochs[] = {0, 1, 2, 3, 17};
        size_t n_epochs = sizeof(epochs) / sizeof(epochs[0]);

        for (size_t e = 0; e < n_epochs; e++) {
                memcpy(evolved_state, state0, statelen);
                memcpy(seek_state, state0, statelen);

                /* Evolve from state0 to target epoch */
                uint64_t current = FSPRG_GetEpoch(evolved_state);
                while (current < epochs[e]) {
                        r = FSPRG_Evolve(evolved_state);
                        if (r < 0)
                                goto finish;
                        current = FSPRG_GetEpoch(evolved_state);
                }

                /* Seek directly to target epoch from state0 using msk+seed */
                r = FSPRG_Seek(seek_state, epochs[e], msk, seed, SEEDLEN);
                if (r < 0)
                        goto finish;

                /* Cross-check: seek and evolve must produce identical state */
                if (memcmp(evolved_state, seek_state, statelen) != 0) {
                        fprintf(stderr, "FSPRG_Seek mismatch at epoch %llu\n",
                                (unsigned long long) epochs[e]);
                        r = -EIO;
                        goto finish;
                }

                hex_encode(hex, evolved_state, statelen);
                fprintf(out, "        {\n");
                fprintf(out, "          \"epoch\": %llu,\n", (unsigned long long) epochs[e]);
                fprintf(out, "          \"state_hex\": \"%s\",\n", hex);

                hex_encode(hex, seek_state, statelen);
                fprintf(out, "          \"seek_state_hex\": \"%s\",\n", hex);
                fprintf(out, "          \"seek_matches_evolved\": true,\n");

                fprintf(out, "          \"keys\": [\n");

                for (uint32_t idx = 0; idx <= 1; idx++) {
                        r = FSPRG_GetKey(evolved_state, key, KEYLEN, idx);
                        if (r < 0)
                                goto finish;
                        hex_encode(hex, key, KEYLEN);
                        fprintf(out, "            {\"idx\": %u, \"keylen\": %u, \"key_hex\": \"%s\"}", idx, (unsigned) KEYLEN, hex);
                        if (idx < 1)
                                fprintf(out, ",");
                        fprintf(out, "\n");
                }

                fprintf(out, "          ]\n");
                fprintf(out, "        }");
                if (e + 1 < n_epochs)
                        fprintf(out, ",");
                fprintf(out, "\n");
        }

        fprintf(out, "      ]\n");
        fprintf(out, "    }");

finish:
        free(msk);
        free(mpk);
        free(state0);
        free(key);
        free(evolved_state);
        free(seek_state);
        free(hex);
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
