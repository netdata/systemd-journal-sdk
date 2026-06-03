#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <systemd/sd-journal.h>

static size_t cstring_len(const char *s) {
    size_t n = 0;

    while (s[n] != '\0')
        n++;
    return n;
}

static double monotonic_seconds(void) {
    struct timespec ts;

    if (clock_gettime(CLOCK_MONOTONIC, &ts) < 0)
        return 0.0;

    return (double) ts.tv_sec + ((double) ts.tv_nsec / 1000000000.0);
}

static void usage(const char *argv0) {
    fprintf(stderr,
            "usage: %s --path PATH --expected N [--match FIELD=VALUE] "
            "[--sequence-field FIELD] [--timeout-sec N]\n",
            argv0);
}

static const char *systemd_error(int r) {
    if (r >= 0)
        return "success";

    return strerror(-r);
}

static int parse_positive_u64(const char *text, uint64_t *value) {
    char *end = NULL;

    errno = 0;
    const unsigned long long parsed = strtoull(text, &end, 10);
    if (errno != 0 || !end || *end != '\0' || parsed == 0 ||
        parsed > UINT64_MAX)
        return -1;

    *value = (uint64_t) parsed;
    return 0;
}

static int parse_positive_double(const char *text, double *value) {
    char *end = NULL;

    errno = 0;
    const double parsed = strtod(text, &end);
    if (errno != 0 || !end || *end != '\0' || parsed <= 0.0)
        return -1;

    *value = parsed;
    return 0;
}

int main(int argc, char **argv) {
    const char *path = NULL;
    const char *match = "PRIORITY=6";
    const char *sequence_field = "LIVE_SEQ";
    uint64_t expected = 0;
    double timeout_sec = 20.0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) {
            path = argv[++i];
        } else if (strcmp(argv[i], "--match") == 0 && i + 1 < argc) {
            match = argv[++i];
        } else if (strcmp(argv[i], "--sequence-field") == 0 && i + 1 < argc) {
            sequence_field = argv[++i];
        } else if (strcmp(argv[i], "--expected") == 0 && i + 1 < argc) {
            if (parse_positive_u64(argv[++i], &expected) < 0) {
                usage(argv[0]);
                return 2;
            }
        } else if (strcmp(argv[i], "--timeout-sec") == 0 && i + 1 < argc) {
            if (parse_positive_double(argv[++i], &timeout_sec) < 0) {
                usage(argv[0]);
                return 2;
            }
        } else {
            usage(argv[0]);
            return 2;
        }
    }

    if (!path || !sequence_field || sequence_field[0] == '\0' ||
        strchr(sequence_field, '=') || expected == 0 || timeout_sec <= 0) {
        usage(argv[0]);
        return 2;
    }

    const size_t sequence_field_len = cstring_len(sequence_field);
    char *sequence_prefix = malloc(sequence_field_len + 2);
    if (!sequence_prefix) {
        fprintf(stderr, "malloc(sequence_prefix): %s\n", strerror(errno));
        return 1;
    }
    memcpy(sequence_prefix, sequence_field, sequence_field_len);
    sequence_prefix[sequence_field_len] = '=';
    sequence_prefix[sequence_field_len + 1] = '\0';
    const size_t sequence_prefix_len = sequence_field_len + 1;

    sd_journal *journal = NULL;
    const char *paths[] = { path, NULL };

    int r = sd_journal_open_files(&journal, (const char **) paths, 0);
    if (r < 0) {
        fprintf(stderr, "sd_journal_open_files(%s): %s\n", path, systemd_error(r));
        free(sequence_prefix);
        return 1;
    }

    r = sd_journal_add_match(journal, match, 0);
    if (r < 0) {
        fprintf(stderr, "sd_journal_add_match(%s): %s\n", match, systemd_error(r));
        sd_journal_close(journal);
        free(sequence_prefix);
        return 1;
    }

    r = sd_journal_set_data_threshold(journal, 0);
    if (r < 0) {
        fprintf(stderr, "sd_journal_set_data_threshold(0): %s\n", systemd_error(r));
        sd_journal_close(journal);
        free(sequence_prefix);
        return 1;
    }

    r = sd_journal_seek_head(journal);
    if (r < 0) {
        fprintf(stderr, "sd_journal_seek_head: %s\n", systemd_error(r));
        sd_journal_close(journal);
        free(sequence_prefix);
        return 1;
    }

    const double deadline = monotonic_seconds() + timeout_sec;
    uint64_t count = 0;
    uint64_t waits = 0;

    while (monotonic_seconds() < deadline) {
        for (;;) {
            r = sd_journal_next(journal);
            if (r < 0) {
                fprintf(stderr, "sd_journal_next after count=%llu: %s\n",
                        (unsigned long long) count, systemd_error(r));
                sd_journal_close(journal);
                free(sequence_prefix);
                return 1;
            }
            if (r == 0)
                break;

            const void *data = NULL;
            size_t size = 0;
            r = sd_journal_get_data(journal, sequence_field, &data, &size);
            if (r < 0) {
                fprintf(stderr, "sd_journal_get_data(%s) after count=%llu: %s\n",
                        sequence_field,
                        (unsigned long long) count, systemd_error(r));
                sd_journal_close(journal);
                free(sequence_prefix);
                return 1;
            }

            if (size <= sequence_prefix_len ||
                memcmp(data, sequence_prefix, sequence_prefix_len) != 0) {
                fprintf(stderr, "unexpected %s payload after count=%llu\n",
                        sequence_field, (unsigned long long) count);
                sd_journal_close(journal);
                free(sequence_prefix);
                return 1;
            }

            const unsigned char *value =
                (const unsigned char *) data + sequence_prefix_len;
            const size_t value_len = size - sequence_prefix_len;
            uint64_t sequence = 0;
            for (size_t j = 0; j < value_len; j++) {
                if (value[j] < '0' || value[j] > '9') {
                    fprintf(stderr, "non-numeric %s payload after count=%llu\n",
                            sequence_field, (unsigned long long) count);
                    sd_journal_close(journal);
                    free(sequence_prefix);
                    return 1;
                }
                sequence = sequence * 10 + (uint64_t) (value[j] - '0');
            }
            if (sequence != count) {
                fprintf(stderr,
                        "out-of-order %s payload: got %llu, expected %llu\n",
                        sequence_field, (unsigned long long) sequence,
                        (unsigned long long) count);
                sd_journal_close(journal);
                free(sequence_prefix);
                return 1;
            }

            count++;
            if (count >= expected) {
                printf("{\"reader\":\"libsystemd\",\"entries\":%llu,\"waits\":%llu}\n",
                       (unsigned long long) count,
                       (unsigned long long) waits);
                sd_journal_close(journal);
                free(sequence_prefix);
                return 0;
            }
        }

        r = sd_journal_wait(journal, 100000);
        if (r < 0) {
            fprintf(stderr, "sd_journal_wait after count=%llu: %s\n",
                    (unsigned long long) count, systemd_error(r));
            sd_journal_close(journal);
            free(sequence_prefix);
            return 1;
        }
        waits++;
    }

    fprintf(stderr, "timeout: observed %llu entries, expected %llu\n",
            (unsigned long long) count,
            (unsigned long long) expected);
    sd_journal_close(journal);
    free(sequence_prefix);
    return 1;
}
