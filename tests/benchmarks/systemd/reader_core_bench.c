/* SPDX-License-Identifier: LGPL-2.1-or-later */

// cppcheck-suppress-file missingIncludeSystem
#include <errno.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>

#include <systemd/sd-journal.h>

static const char *arg_mode = "data";
static const char *arg_surface = "file";
static const char *arg_direction = "forward";
static const char **arg_inputs = NULL;
static size_t arg_inputs_count = 0;

typedef struct Counts {
        uint64_t records;
        uint64_t fields;
        uint64_t bytes;
        uint64_t checksum;
} Counts;

static double elapsed_seconds(struct timespec start, struct timespec end) {
        return (double) (end.tv_sec - start.tv_sec) +
               (double) (end.tv_nsec - start.tv_nsec) / 1000000000.0;
}

static void checksum_payload(Counts *counts, const void *data, size_t len) {
        const uint8_t *bytes = data;
        counts->fields++;
        counts->bytes += len;
        counts->checksum = (counts->checksum << 5) | (counts->checksum >> 59);
        counts->checksum ^= (uint64_t) len;
        if (len > 0) {
                counts->checksum ^= (uint64_t) bytes[0] << 8;
                counts->checksum ^= (uint64_t) bytes[len - 1];
        }
}

static void record_marker(Counts *counts, uint64_t value) {
        counts->records++;
        counts->checksum = (counts->checksum << 7) | (counts->checksum >> 57);
        counts->checksum ^= value;
}

static int parse_args(int argc, char **argv) {
        arg_inputs = calloc((size_t) argc + 1, sizeof(char *));
        if (!arg_inputs)
                return -ENOMEM;

        for (int i = 1; i < argc; i++) {
                if (strcmp(argv[i], "--input") == 0 && i + 1 < argc)
                        arg_inputs[arg_inputs_count++] = argv[++i];
                else if (strcmp(argv[i], "--mode") == 0 && i + 1 < argc)
                        arg_mode = argv[++i];
                else if (strcmp(argv[i], "--surface") == 0 && i + 1 < argc)
                        arg_surface = argv[++i];
                else if (strcmp(argv[i], "--direction") == 0 && i + 1 < argc)
                        arg_direction = argv[++i];
                else
                        return -EINVAL;
        }

        if (arg_inputs_count == 0)
                return -EINVAL;
        if (strcmp(arg_mode, "next") != 0 && strcmp(arg_mode, "data") != 0)
                return -EINVAL;
        if (strcmp(arg_surface, "file") != 0 &&
            strcmp(arg_surface, "open-files") != 0 &&
            strcmp(arg_surface, "directory") != 0)
                return -EINVAL;
        if (strcmp(arg_direction, "forward") != 0 && strcmp(arg_direction, "backward") != 0)
                return -EINVAL;
        if (strcmp(arg_surface, "file") == 0 && arg_inputs_count != 1)
                return -EINVAL;
        if (strcmp(arg_surface, "directory") == 0 && arg_inputs_count != 1)
                return -EINVAL;

        return 0;
}

static int open_journal(sd_journal **ret) {
        int r;

        if (strcmp(arg_surface, "directory") == 0)
                r = sd_journal_open_directory(ret, arg_inputs[0], 0);
        else
                r = sd_journal_open_files(ret, arg_inputs, 0);

        if (r < 0)
                fprintf(stderr, "failed to open journal: %s\n", strerror(-r));
        return r;
}

static int read_journal(sd_journal *j, Counts *counts) {
        int r;

        if (strcmp(arg_direction, "backward") == 0)
                r = sd_journal_seek_tail(j);
        else
                r = sd_journal_seek_head(j);
        if (r < 0)
                return r;

        for (;;) {
                r = strcmp(arg_direction, "backward") == 0 ? sd_journal_previous(j) : sd_journal_next(j);
                if (r < 0)
                        return r;
                if (r == 0)
                        return 0;

                uint64_t realtime = 0;
                r = sd_journal_get_realtime_usec(j, &realtime);
                if (r < 0)
                        return r;
                record_marker(counts, realtime);

                if (strcmp(arg_mode, "next") == 0)
                        continue;

                const void *data = NULL;
                size_t len = 0;
                sd_journal_restart_data(j);
                SD_JOURNAL_FOREACH_DATA(j, data, len)
                        checksum_payload(counts, data, len);
        }
}

static uint64_t rss_kb(void) {
        struct rusage usage = {};
        if (getrusage(RUSAGE_SELF, &usage) < 0)
                return 0;
        return (uint64_t) usage.ru_maxrss;
}

int main(int argc, char **argv) {
        int r = parse_args(argc, argv);
        if (r < 0) {
                fprintf(stderr, "usage: %s --input PATH [--input PATH...] --surface file|open-files|directory --mode next|data --direction forward|backward\n", argv[0]);
                free(arg_inputs);
                return 2;
        }

        sd_journal *j = NULL;
        r = open_journal(&j);
        if (r < 0) {
            free(arg_inputs);
            return 1;
        }

        Counts counts = {};
        uint64_t rss_before = rss_kb();
        struct timespec start, end;
        clock_gettime(CLOCK_MONOTONIC, &start);
        r = read_journal(j, &counts);
        clock_gettime(CLOCK_MONOTONIC, &end);
        uint64_t rss_after = rss_kb();
        sd_journal_close(j);

        if (r < 0) {
                fprintf(stderr, "read failed: %s\n", strerror(-r));
                free(arg_inputs);
                return 1;
        }

        double seconds = elapsed_seconds(start, end);
        printf("{\"language\":\"systemd\",\"surface\":\"%s\",\"mode\":\"%s\",\"direction\":\"%s\","
               "\"records\":%" PRIu64 ",\"fields\":%" PRIu64 ",\"bytes\":%" PRIu64 ","
               "\"checksum\":%" PRIu64 ",\"read_seconds\":%.9f,"
               "\"read_rows_per_second\":%.3f,\"read_fields_per_second\":%.3f,"
               "\"read_bytes_per_second\":%.3f,\"max_rss_before_kb\":%" PRIu64 ","
               "\"max_rss_after_kb\":%" PRIu64 ",\"errors\":[]}\n",
               arg_surface,
               arg_mode,
               arg_direction,
               counts.records,
               counts.fields,
               counts.bytes,
               counts.checksum,
               seconds,
               seconds > 0 ? (double) counts.records / seconds : 0,
               seconds > 0 ? (double) counts.fields / seconds : 0,
               seconds > 0 ? (double) counts.bytes / seconds : 0,
               rss_before,
               rss_after);

        free(arg_inputs);
        return 0;
}
