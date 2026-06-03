// SPDX-License-Identifier: GPL-3.0-or-later
//
// Test helper for byte-for-byte journal field readback through stock
// libsystemd. This is intentionally a standalone validation helper, not an SDK
// dependency.

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

int main(int argc, char **argv) {
        const char *paths[2];
        sd_journal *j = NULL;
        uint8_t *expected = NULL;
        size_t expected_len = 0;
        const void *data = NULL;
        size_t data_len = 0;
        size_t field_len;
        int r;

        if (argc < 4) {
                fprintf(stderr, "usage: %s JOURNAL FIELD EXPECTED_HEX [MATCH...]\n", argv[0]);
                return 2;
        }

        field_len = cstring_len(argv[2]);
        if (field_len == 0) {
                fprintf(stderr, "field name must not be empty\n");
                return 2;
        }

        if (parse_hex(argv[3], &expected, &expected_len) < 0) {
                fprintf(stderr, "invalid expected hex\n");
                return 2;
        }

        paths[0] = argv[1];
        paths[1] = NULL;
        r = sd_journal_open_files(&j, paths, 0);
        if (r < 0) {
                fprintf(stderr, "sd_journal_open_files failed: %d\n", r);
                free(expected);
                return 1;
        }

        r = sd_journal_set_data_threshold(j, 0);
        if (r < 0) {
                fprintf(stderr, "sd_journal_set_data_threshold failed: %d\n", r);
                sd_journal_close(j);
                free(expected);
                return 1;
        }

        for (int i = 4; i < argc; i++) {
                r = sd_journal_add_match(j, argv[i], cstring_len(argv[i]));
                if (r < 0) {
                        fprintf(stderr, "sd_journal_add_match(%s) failed: %d\n", argv[i], r);
                        sd_journal_close(j);
                        free(expected);
                        return 1;
                }
        }

        r = sd_journal_seek_head(j);
        if (r < 0) {
                fprintf(stderr, "sd_journal_seek_head failed: %d\n", r);
                sd_journal_close(j);
                free(expected);
                return 1;
        }

        r = sd_journal_next(j);
        if (r < 0) {
                fprintf(stderr, "sd_journal_next failed: %d\n", r);
                sd_journal_close(j);
                free(expected);
                return 1;
        }
        if (r == 0) {
                fprintf(stderr, "no matching entry found\n");
                sd_journal_close(j);
                free(expected);
                return 1;
        }

        r = sd_journal_get_data(j, argv[2], &data, &data_len);
        if (r < 0) {
                fprintf(stderr, "sd_journal_get_data(%s) failed: %d\n", argv[2], r);
                sd_journal_close(j);
                free(expected);
                return 1;
        }

        if (data_len != field_len + 1 + expected_len ||
            memcmp(data, argv[2], field_len) != 0 ||
            ((const uint8_t *) data)[field_len] != '=' ||
            (expected_len > 0 &&
             memcmp((const uint8_t *) data + field_len + 1, expected, expected_len) != 0)) {
                fprintf(stderr, "field bytes mismatch: got %zu bytes, expected %zu-byte value\n",
                        data_len, expected_len);
                sd_journal_close(j);
                free(expected);
                return 1;
        }

        sd_journal_close(j);
        free(expected);
        return 0;
}
