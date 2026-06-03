// cppcheck-suppress-file missingIncludeSystem
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

typedef struct LiveReaderConfig {
    const char *path;
    const char *match;
    const char *sequence_field;
    uint64_t expected;
    double timeout_sec;
} LiveReaderConfig;

typedef struct LiveReaderRuntime {
    sd_journal *journal;
    char *sequence_prefix;
    size_t sequence_prefix_len;
    uint64_t count;
    uint64_t waits;
    double deadline;
} LiveReaderRuntime;

static const char **string_option_target(const char *arg, LiveReaderConfig *config) {
    if (strcmp(arg, "--path") == 0)
        return &config->path;
    if (strcmp(arg, "--match") == 0)
        return &config->match;
    if (strcmp(arg, "--sequence-field") == 0)
        return &config->sequence_field;
    return NULL;
}

static int parse_string_option(char **argv, int argc, int *idx, LiveReaderConfig *config) {
    const char **target = string_option_target(argv[*idx], config);
    if (!target)
        return 0;
    if (*idx + 1 >= argc)
        return -1;
    *target = argv[++(*idx)];
    return 1;
}

static int parse_expected_option(char **argv, int argc, int *idx, LiveReaderConfig *config) {
    if (strcmp(argv[*idx], "--expected") != 0)
        return 0;
    if (*idx + 1 >= argc)
        return -1;
    return parse_positive_u64(argv[++(*idx)], &config->expected) < 0 ? -1 : 1;
}

static int parse_timeout_option(char **argv, int argc, int *idx, LiveReaderConfig *config) {
    if (strcmp(argv[*idx], "--timeout-sec") != 0)
        return 0;
    if (*idx + 1 >= argc)
        return -1;
    return parse_positive_double(argv[++(*idx)], &config->timeout_sec) < 0 ? -1 : 1;
}

static int parse_one_arg(char **argv, int argc, int *idx, LiveReaderConfig *config) {
    int parsed = parse_string_option(argv, argc, idx, config);
    if (parsed != 0)
        return parsed;
    parsed = parse_expected_option(argv, argc, idx, config);
    if (parsed != 0)
        return parsed;
    return parse_timeout_option(argv, argc, idx, config);
}

static int parse_args(int argc, char **argv, LiveReaderConfig *config) {
    for (int i = 1; i < argc; i++) {
        int parsed = parse_one_arg(argv, argc, &i, config);
        if (parsed <= 0)
            return 2;
    }
    return 0;
}

static int validate_config(const LiveReaderConfig *config) {
    return config->path &&
           config->sequence_field &&
           config->sequence_field[0] != '\0' &&
           !strchr(config->sequence_field, '=') &&
           config->expected > 0 &&
           config->timeout_sec > 0;
}

static int init_sequence_prefix(const LiveReaderConfig *config, LiveReaderRuntime *runtime) {
    const size_t sequence_field_len = cstring_len(config->sequence_field);
    runtime->sequence_prefix = malloc(sequence_field_len + 2);
    if (!runtime->sequence_prefix) {
        fprintf(stderr, "malloc(sequence_prefix): %s\n", strerror(errno));
        return 1;
    }
    memcpy(runtime->sequence_prefix, config->sequence_field, sequence_field_len);
    runtime->sequence_prefix[sequence_field_len] = '=';
    runtime->sequence_prefix[sequence_field_len + 1] = '\0';
    runtime->sequence_prefix_len = sequence_field_len + 1;
    return 0;
}

static int open_configured_journal(const LiveReaderConfig *config, LiveReaderRuntime *runtime) {
    const char *paths[] = { config->path, NULL };
    int r = sd_journal_open_files(&runtime->journal, (const char **) paths, 0);
    if (r < 0) {
        fprintf(stderr, "sd_journal_open_files(%s): %s\n", config->path, systemd_error(r));
        return 1;
    }

    r = sd_journal_add_match(runtime->journal, config->match, 0);
    if (r < 0) {
        fprintf(stderr, "sd_journal_add_match(%s): %s\n", config->match, systemd_error(r));
        return 1;
    }

    r = sd_journal_set_data_threshold(runtime->journal, 0);
    if (r < 0) {
        fprintf(stderr, "sd_journal_set_data_threshold(0): %s\n", systemd_error(r));
        return 1;
    }

    r = sd_journal_seek_head(runtime->journal);
    if (r < 0) {
        fprintf(stderr, "sd_journal_seek_head: %s\n", systemd_error(r));
        return 1;
    }
    return 0;
}

static int parse_sequence_value(
    const LiveReaderConfig *config,
    const unsigned char *value,
    size_t value_len,
    uint64_t *sequence
) {
    *sequence = 0;
    for (size_t i = 0; i < value_len; i++) {
        if (value[i] < '0' || value[i] > '9') {
            fprintf(stderr, "non-numeric %s payload\n", config->sequence_field);
            return 1;
        }
        *sequence = *sequence * 10 + (uint64_t) (value[i] - '0');
    }
    return 0;
}

static int read_sequence(
    const LiveReaderConfig *config,
    LiveReaderRuntime *runtime,
    uint64_t *sequence
) {
    const void *data = NULL;
    size_t size = 0;
    int r = sd_journal_get_data(runtime->journal, config->sequence_field, &data, &size);
    if (r < 0) {
        fprintf(stderr, "sd_journal_get_data(%s) after count=%llu: %s\n",
                config->sequence_field,
                (unsigned long long) runtime->count,
                systemd_error(r));
        return 1;
    }

    if (size <= runtime->sequence_prefix_len ||
        memcmp(data, runtime->sequence_prefix, runtime->sequence_prefix_len) != 0) {
        fprintf(stderr, "unexpected %s payload after count=%llu\n",
                config->sequence_field, (unsigned long long) runtime->count);
        return 1;
    }

    return parse_sequence_value(
        config,
        (const unsigned char *) data + runtime->sequence_prefix_len,
        size - runtime->sequence_prefix_len,
        sequence
    );
}

static int validate_sequence(const LiveReaderConfig *config, LiveReaderRuntime *runtime) {
    uint64_t sequence = 0;
    if (read_sequence(config, runtime, &sequence) != 0)
        return 1;
    if (sequence != runtime->count) {
        fprintf(stderr,
                "out-of-order %s payload: got %llu, expected %llu\n",
                config->sequence_field,
                (unsigned long long) sequence,
                (unsigned long long) runtime->count);
        return 1;
    }
    runtime->count++;
    return 0;
}

static int drain_available_entries(const LiveReaderConfig *config, LiveReaderRuntime *runtime) {
    for (;;) {
        int r = sd_journal_next(runtime->journal);
        if (r < 0) {
            fprintf(stderr, "sd_journal_next after count=%llu: %s\n",
                    (unsigned long long) runtime->count, systemd_error(r));
            return 1;
        }
        if (r == 0)
            return 0;
        if (validate_sequence(config, runtime) != 0)
            return 1;
        if (runtime->count >= config->expected)
            return 0;
    }
}

static int wait_for_entries(LiveReaderRuntime *runtime) {
    int r = sd_journal_wait(runtime->journal, 100000);
    if (r < 0) {
        fprintf(stderr, "sd_journal_wait after count=%llu: %s\n",
                (unsigned long long) runtime->count, systemd_error(r));
        return 1;
    }
    runtime->waits++;
    return 0;
}

static int run_reader_loop(const LiveReaderConfig *config, LiveReaderRuntime *runtime) {
    while (monotonic_seconds() < runtime->deadline) {
        if (drain_available_entries(config, runtime) != 0)
            return 1;
        if (runtime->count >= config->expected)
            return 0;
        if (wait_for_entries(runtime) != 0)
            return 1;
    }
    fprintf(stderr, "timeout: observed %llu entries, expected %llu\n",
            (unsigned long long) runtime->count,
            (unsigned long long) config->expected);
    return 1;
}

static void close_runtime(LiveReaderRuntime *runtime) {
    if (runtime->journal)
        sd_journal_close(runtime->journal);
    free(runtime->sequence_prefix);
}

int main(int argc, char **argv) {
    LiveReaderConfig config = {0};
    config.match = "PRIORITY=6";
    config.sequence_field = "LIVE_SEQ";
    config.timeout_sec = 20.0;
    LiveReaderRuntime runtime = {0};

    int r = parse_args(argc, argv, &config);
    if (r != 0) {
        usage(argv[0]);
        return r < 0 ? 2 : r;
    }
    if (!validate_config(&config)) {
        usage(argv[0]);
        return 2;
    }
    if (init_sequence_prefix(&config, &runtime) != 0)
        return 1;
    if (open_configured_journal(&config, &runtime) != 0) {
        close_runtime(&runtime);
        return 1;
    }

    runtime.deadline = monotonic_seconds() + config.timeout_sec;
    r = run_reader_loop(&config, &runtime);
    if (r == 0)
        printf("{\"reader\":\"libsystemd\",\"entries\":%llu,\"waits\":%llu}\n",
               (unsigned long long) runtime.count,
               (unsigned long long) runtime.waits);
    close_runtime(&runtime);
    return r;
}
