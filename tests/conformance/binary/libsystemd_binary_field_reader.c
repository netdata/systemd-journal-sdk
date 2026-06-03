// SPDX-License-Identifier: GPL-3.0-or-later
//
// Test helper for byte-for-byte journal field readback through stock
// libsystemd. This is intentionally a standalone validation helper, not an SDK
// dependency.

// cppcheck-suppress-file missingIncludeSystem
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <systemd/sd-journal.h>

static size_t cstring_len(const char *s) {
        size_t n = 0;

        while (s[n] != '\0')
                n++;
        return n;
}

static int hex_value(char c) {
        if (c >= '0' && c <= '9')
                return c - '0';
        if (c >= 'a' && c <= 'f')
                return c - 'a' + 10;
        if (c >= 'A' && c <= 'F')
                return c - 'A' + 10;
        return -1;
}

static int parse_hex(const char *hex, uint8_t **ret, size_t *ret_len) {
        size_t n = cstring_len(hex);
        uint8_t *buf;

        if (n % 2 != 0)
                return -1;

        buf = malloc(n / 2);
        if (!buf && n != 0)
                return -1;

        for (size_t i = 0; i < n; i += 2) {
                int hi = hex_value(hex[i]);
                int lo = hex_value(hex[i + 1]);

                if (hi < 0 || lo < 0) {
                        free(buf);
                        return -1;
                }

                buf[i / 2] = (uint8_t) ((hi << 4) | lo);
        }

        *ret = buf;
        *ret_len = n / 2;
        return 0;
}

typedef struct BinaryReaderConfig {
        const char *journal_path;
        const char *field;
        char **matches;
        int n_matches;
        uint8_t *expected;
        size_t expected_len;
        size_t field_len;
} BinaryReaderConfig;

static void usage(const char *argv0) {
        fprintf(stderr, "usage: %s JOURNAL FIELD EXPECTED_HEX [MATCH...]\n", argv0);
}

static int parse_config(int argc, char **argv, BinaryReaderConfig *config) {
        if (argc < 4) {
                usage(argv[0]);
                return 2;
        }

        config->journal_path = argv[1];
        config->field = argv[2];
        config->matches = argc > 4 ? &argv[4] : NULL;
        config->n_matches = argc - 4;
        config->field_len = cstring_len(config->field);
        if (config->field_len == 0) {
                fprintf(stderr, "field name must not be empty\n");
                return 2;
        }

        if (parse_hex(argv[3], &config->expected, &config->expected_len) < 0) {
                fprintf(stderr, "invalid expected hex\n");
                return 2;
        }
        return 0;
}

static int open_binary_journal(const BinaryReaderConfig *config, sd_journal **journal) {
        const char *paths[2];
        paths[0] = config->journal_path;
        paths[1] = NULL;
        int r = sd_journal_open_files(journal, paths, 0);
        if (r < 0) {
                fprintf(stderr, "sd_journal_open_files failed: %d\n", r);
                return 1;
        }
        return 0;
}

static int configure_binary_journal(sd_journal *journal) {
        int r = sd_journal_set_data_threshold(journal, 0);
        if (r < 0) {
                fprintf(stderr, "sd_journal_set_data_threshold failed: %d\n", r);
                return 1;
        }
        return 0;
}

static int add_matches(sd_journal *journal, const BinaryReaderConfig *config) {
        for (int i = 0; i < config->n_matches; i++) {
                const char *match = config->matches[i];
                int r = sd_journal_add_match(journal, match, cstring_len(match));
                if (r < 0) {
                        fprintf(stderr, "sd_journal_add_match(%s) failed: %d\n", match, r);
                        return 1;
                }
        }
        return 0;
}

static int seek_first_match(sd_journal *journal) {
        int r = sd_journal_seek_head(journal);
        if (r < 0) {
                fprintf(stderr, "sd_journal_seek_head failed: %d\n", r);
                return 1;
        }

        r = sd_journal_next(journal);
        if (r < 0) {
                fprintf(stderr, "sd_journal_next failed: %d\n", r);
                return 1;
        }
        if (r == 0) {
                fprintf(stderr, "no matching entry found\n");
                return 1;
        }
        return 0;
}

static int read_field(sd_journal *journal, const BinaryReaderConfig *config, const void **data, size_t *data_len) {
        int r = sd_journal_get_data(journal, config->field, data, data_len);
        if (r < 0) {
                fprintf(stderr, "sd_journal_get_data(%s) failed: %d\n", config->field, r);
                return 1;
        }
        return 0;
}

static int field_prefix_matches(const BinaryReaderConfig *config, const void *data, size_t data_len) {
        return data_len == config->field_len + 1 + config->expected_len &&
               memcmp(data, config->field, config->field_len) == 0 &&
               ((const uint8_t *) data)[config->field_len] == '=';
}

static int field_value_matches(const BinaryReaderConfig *config, const void *data) {
        return config->expected_len == 0 ||
               memcmp((const uint8_t *) data + config->field_len + 1,
                      config->expected,
                      config->expected_len) == 0;
}

static int verify_field_bytes(const BinaryReaderConfig *config, const void *data, size_t data_len) {
        if (!field_prefix_matches(config, data, data_len) || !field_value_matches(config, data)) {
                fprintf(stderr, "field bytes mismatch: got %zu bytes, expected %zu-byte value\n",
                        data_len, config->expected_len);
                return 1;
        }
        return 0;
}

static int run_binary_reader(const BinaryReaderConfig *config) {
        sd_journal *journal = NULL;
        const void *data = NULL;
        size_t data_len = 0;
        int r = open_binary_journal(config, &journal);
        if (r == 0)
                r = configure_binary_journal(journal);
        if (r == 0)
                r = add_matches(journal, config);
        if (r == 0)
                r = seek_first_match(journal);
        if (r == 0)
                r = read_field(journal, config, &data, &data_len);
        if (r == 0)
                r = verify_field_bytes(config, data, data_len);
        if (journal)
                sd_journal_close(journal);
        return r;
}

int main(int argc, char **argv) {
        BinaryReaderConfig config = {0};
        int r = parse_config(argc, argv, &config);
        if (r == 0)
                r = run_binary_reader(&config);
        free(config.expected);
        return r;
}
