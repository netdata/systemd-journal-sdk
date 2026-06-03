/* SPDX-License-Identifier: LGPL-2.1-or-later */

// cppcheck-suppress-file missingIncludeSystem
#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/uio.h>
#include <time.h>
#include <unistd.h>

#include "sd-id128.h"

#include "journal-file.h"
#include "journal-file-util.h"
#include "log.h"
#include "mmap-cache.h"
#include "string-util.h"
#include "time-util.h"

#define FIELDS_PER_ROW 32U

static const uint64_t base_realtime_usec = 1700000000000000ULL;
static const uint64_t base_monotonic_usec = 50000000ULL;
static const char *seqnum_id_hex = "22222222222222222222222222222222";

static const char *arg_output = NULL;
static const char *arg_format = "compact";
static const char *arg_final_state = "online";
static const char *arg_surface = "direct";
static size_t arg_rows = 100000;
static uint64_t arg_max_size = 128ULL * 1024ULL * 1024ULL;
static uint64_t arg_rotation_max_size = 128ULL * 1024ULL * 1024ULL;
static char arg_archived_output[PATH_MAX];

typedef struct BenchRows {
        struct iovec *iovecs;
        size_t rows;
} BenchRows;

static double elapsed_seconds(struct timespec start, struct timespec end) {
        return (double) (end.tv_sec - start.tv_sec) +
               (double) (end.tv_nsec - start.tv_nsec) / 1000000000.0;
}

static int parse_positive_ull(const char *text, unsigned long long *ret) {
        char *end = NULL;
        errno = 0;
        unsigned long long value = strtoull(text, &end, 10);
        if (errno != 0 || !end || *end != '\0' || value == 0)
                return -EINVAL;
        *ret = value;
        return 0;
}

static int parse_unsigned_ull(const char *text, unsigned long long *ret) {
        char *end = NULL;
        errno = 0;
        unsigned long long value = strtoull(text, &end, 10);
        if (errno != 0 || !end || *end != '\0')
                return -EINVAL;
        *ret = value;
        return 0;
}

static int parse_rows_value(const char *text) {
        unsigned long long value;
        int r = parse_positive_ull(text, &value);
        if (r < 0)
                return r;
        arg_rows = (size_t) value;
        return 0;
}

static int parse_max_size_value(const char *text, uint64_t *target) {
        unsigned long long value;
        int r = parse_positive_ull(text, &value);
        if (r < 0)
                return r;
        *target = (uint64_t) value;
        return 0;
}

static int parse_ignored_live_publish_value(const char *text) {
        unsigned long long value;
        return parse_unsigned_ull(text, &value);
}

static int parse_string_arg(const char *arg, const char *value) {
        if (streq(arg, "--output")) {
                arg_output = value;
                return 1;
        }
        if (streq(arg, "--format")) {
                arg_format = value;
                return 1;
        }
        if (streq(arg, "--final-state")) {
                arg_final_state = value;
                return 1;
        }
        if (streq(arg, "--surface")) {
                arg_surface = value;
                return 1;
        }
        return 0;
}

static int parse_api_mode_arg(const char *value) {
        return streq(value, "raw-payload") ? 0 : -EINVAL;
}

static int parse_option_with_value(int argc, char **argv, int *idx, int (*handler)(const char *)) {
        if (*idx + 1 >= argc)
                return -EINVAL;
        return handler(argv[++(*idx)]);
}

static int parse_max_size_arg(const char *text);
static int parse_rotation_max_size_arg(const char *text);

static int parse_one_arg(int argc, char **argv, int *idx) {
        const char *arg = argv[*idx];
        if (*idx + 1 < argc && parse_string_arg(arg, argv[*idx + 1])) {
                (*idx)++;
                return 0;
        }
        if (streq(arg, "--rows"))
                return parse_option_with_value(argc, argv, idx, parse_rows_value);
        if (streq(arg, "--max-size-bytes"))
                return parse_option_with_value(argc, argv, idx, parse_max_size_arg);
        if (streq(arg, "--rotation-max-size-bytes"))
                return parse_option_with_value(argc, argv, idx, parse_rotation_max_size_arg);
        if (streq(arg, "--api-mode"))
                return parse_option_with_value(argc, argv, idx, parse_api_mode_arg);
        if (streq(arg, "--live-publish-every-entries"))
                return parse_option_with_value(argc, argv, idx, parse_ignored_live_publish_value);
        return -EINVAL;
}

static int parse_max_size_arg(const char *text) {
        return parse_max_size_value(text, &arg_max_size);
}

static int parse_rotation_max_size_arg(const char *text) {
        return parse_max_size_value(text, &arg_rotation_max_size);
}

static int validate_args(void) {
        if (!arg_output)
                return -EINVAL;
        if (!streq(arg_format, "compact") && !streq(arg_format, "regular"))
                return -EINVAL;
        if (!streq(arg_final_state, "online") && !streq(arg_final_state, "offline") && !streq(arg_final_state, "archived"))
                return -EINVAL;
        if (!streq(arg_surface, "direct") && !streq(arg_surface, "directory"))
                return -EINVAL;
        return 0;
}

static int parse_args(int argc, char **argv) {
        for (int i = 1; i < argc; i++) {
                int r = parse_one_arg(argc, argv, &i);
                if (r < 0)
                        return r;
        }
        return validate_args();
}

static int id128_from_string(const char *s, sd_id128_t *ret) {
        int r;

        r = sd_id128_from_string(s, ret);
        if (r < 0)
                fprintf(stderr, "invalid id128 '%s': %s\n", s, strerror(-r));
        return r;
}

static int configure_header(JournalFile *f) {
        sd_id128_t file_id, machine_id, seqnum_id, boot_id;
        int r;

        r = id128_from_string("33333333333333333333333333333333", &file_id);
        if (r < 0)
                return r;
        r = id128_from_string("fedcba9876543210fedcba9876543210", &machine_id);
        if (r < 0)
                return r;
        r = id128_from_string("22222222222222222222222222222222", &seqnum_id);
        if (r < 0)
                return r;
        r = id128_from_string("0123456789abcdef0123456789abcdef", &boot_id);
        if (r < 0)
                return r;

        f->header->file_id = file_id;
        f->header->machine_id = machine_id;
        f->header->seqnum_id = seqnum_id;
        f->header->tail_entry_boot_id = boot_id;
        return 0;
}

static int open_journal(const char *path, uint64_t max_size, MMapCache **ret_cache, JournalFile **ret_file) {
        JournalMetrics metrics;
        MMapCache *cache;
        JournalFile *file = NULL;
        int r;

        (void) setenv("SYSTEMD_JOURNAL_COMPRESS", "0", 1);
        (void) setenv("SYSTEMD_JOURNAL_COMPACT", streq(arg_format, "compact") ? "1" : "0", 1);
        (void) setenv("SYSTEMD_JOURNAL_KEYED_HASH", "1", 1);

        cache = mmap_cache_new();
        if (!cache)
                return -ENOMEM;

        journal_reset_metrics(&metrics);
        metrics.max_size = max_size;
        metrics.keep_free = 0;

        (void) unlink(path);
        r = journal_file_open(
                        -EBADF,
                        path,
                        O_RDWR | O_CREAT,
                        0,
                        0644,
                        UINT64_MAX,
                        &metrics,
                        cache,
                        NULL,
                        &file);
        if (r < 0) {
                mmap_cache_unref(cache);
                return r;
        }

        r = configure_header(file);
        if (r < 0) {
                journal_file_close(file);
                mmap_cache_unref(cache);
                return r;
        }

        *ret_cache = cache;
        *ret_file = file;
        return 0;
}

static int close_journal(MMapCache *cache, JournalFile *file) {
        int r = 0;

        if (file) {
                if (streq(arg_final_state, "archived")) {
                        r = journal_file_archive(file, NULL);
                        if (r < 0)
                                journal_file_close(file);
                        else
                                journal_file_offline_close(file);
                } else if (streq(arg_final_state, "offline"))
                        journal_file_offline_close(file);
                else {
                        (void) journal_file_set_offline_thread_join(file);
                        journal_file_close(file);
                }
        }
        if (cache)
                mmap_cache_unref(cache);
        return r;
}

static int make_payload(char **ret, size_t *ret_size, const char *name, const char *value) {
        int r;

        r = asprintf(ret, "%s=%s", name, value);
        if (r < 0)
                return -ENOMEM;
        *ret_size = (size_t) r;
        return 0;
}

static int set_payload(struct iovec *iov, const char *name, const char *value) {
        char *payload = NULL;
        size_t payload_size = 0;
        int r;

        r = make_payload(&payload, &payload_size, name, value);
        if (r < 0)
                return r;
        *iov = (struct iovec) {
                .iov_base = payload,
                .iov_len = payload_size,
        };
        return 0;
}

static void free_iovec_payloads(struct iovec *iovecs, size_t count) {
        for (size_t i = 0; i < count; i++)
                free(iovecs[i].iov_base);
}

static int build_rows(size_t rows, BenchRows *ret) {
        struct iovec *iovecs;
        size_t total_iovecs;
        static const char *fixed_names[] = {
                "TEST_ID",
                "PERF_PROFILE",
                "HOST_CLASS",
                "SOURCE_KIND",
        };
        static const char *fixed_values[] = {
                "deterministic-ingestion-performance",
                "mixed-cardinality-32-fields",
                "synthetic-edge",
                "journal-sdk-benchmark",
        };

        if (rows > SIZE_MAX / FIELDS_PER_ROW)
                return -EOVERFLOW;

        total_iovecs = rows * FIELDS_PER_ROW;
        iovecs = calloc(total_iovecs, sizeof(struct iovec));
        if (!iovecs)
                return -ENOMEM;

        for (size_t row = 0; row < rows; row++) {
                struct iovec *record = iovecs + row * FIELDS_PER_ROW;
                size_t pos = 0;

                for (size_t i = 0; i < 4; i++) {
                        int r = set_payload(&record[pos++], fixed_names[i], fixed_values[i]);
                        if (r < 0) {
                                free_iovec_payloads(iovecs, total_iovecs);
                                free(iovecs);
                                return r;
                        }
                }
                for (size_t offset = 0; offset < 12; offset++) {
                        char name[32], value[32];
                        snprintf(name, sizeof(name), "LOW_CARD_%02zu", offset);
                        snprintf(value, sizeof(value), "low-%02zu-%02zu", offset, row % 16);
                        int r = set_payload(&record[pos++], name, value);
                        if (r < 0) {
                                free_iovec_payloads(iovecs, total_iovecs);
                                free(iovecs);
                                return r;
                        }
                }
                for (size_t offset = 0; offset < 8; offset++) {
                        char name[32], value[32];
                        snprintf(name, sizeof(name), "MED_CARD_%02zu", offset);
                        snprintf(value, sizeof(value), "medium-%02zu-%04zu", offset, row % 2048);
                        int r = set_payload(&record[pos++], name, value);
                        if (r < 0) {
                                free_iovec_payloads(iovecs, total_iovecs);
                                free(iovecs);
                                return r;
                        }
                }
                for (size_t offset = 0; offset < 8; offset++) {
                        char name[32], value[32];
                        snprintf(name, sizeof(name), "HIGH_CARD_%02zu", offset);
                        snprintf(value, sizeof(value), "high-%02zu-%06zu", offset, row);
                        int r = set_payload(&record[pos++], name, value);
                        if (r < 0) {
                                free_iovec_payloads(iovecs, total_iovecs);
                                free(iovecs);
                                return r;
                        }
                }
        }

        *ret = (BenchRows) {
                .iovecs = iovecs,
                .rows = rows,
        };
        return 0;
}

static void free_rows(BenchRows *rows) {
        if (!rows || !rows->iovecs)
                return;
        free_iovec_payloads(rows->iovecs, rows->rows * FIELDS_PER_ROW);
        free(rows->iovecs);
        rows->iovecs = NULL;
        rows->rows = 0;
}

static const char *journal_path_after_close(void) {
        char *suffix;
        size_t prefix_len;
        int n;

        if (!streq(arg_final_state, "archived"))
                return arg_output;
        if (arg_archived_output[0] != '\0')
                return arg_archived_output;

        n = snprintf(arg_archived_output, sizeof(arg_archived_output), "%s", arg_output);
        if (n < 0 || (size_t) n >= sizeof(arg_archived_output))
                return arg_output;

        suffix = endswith(arg_archived_output, ".journal");
        if (suffix) {
                *suffix = '\0';
                prefix_len = (size_t) (suffix - arg_archived_output);
        } else
                prefix_len = (size_t) n;
        n = snprintf(arg_archived_output + prefix_len,
                     sizeof(arg_archived_output) - prefix_len,
                     "@%s-%016" PRIx64 "-%016" PRIx64 ".journal",
                     seqnum_id_hex,
                     UINT64_C(1),
                     base_realtime_usec);
        if (n < 0 || prefix_len + (size_t) n >= sizeof(arg_archived_output))
                return arg_output;
        return arg_archived_output;
}

static uint64_t file_size_or_zero(const char *path) {
        struct stat st;

        if (stat(path, &st) < 0)
                return 0;
        return (uint64_t) st.st_size;
}

static uint64_t data_hash_buckets_for_max_size(uint64_t max_size) {
        uint64_t buckets;

        buckets = max_size / 576;
        return buckets > UINT64_C(2047) ? buckets : UINT64_C(2047);
}

static int mkdir_if_needed(const char *path) {
        if (mkdir(path, 0755) >= 0)
                return 0;
        if (errno == EEXIST)
                return 0;
        return -errno;
}

static int make_directory_path(char *ret, size_t size) {
        int n;

        n = snprintf(ret, size, "%s/fedcba9876543210fedcba9876543210", arg_output);
        if (n < 0 || (size_t) n >= size)
                return -ENAMETOOLONG;
        return 0;
}

static int make_active_path(char *ret, size_t size) {
        char dir[PATH_MAX];
        int r, n;

        r = make_directory_path(dir, sizeof(dir));
        if (r < 0)
                return r;
        n = snprintf(ret, size, "%s/system.journal", dir);
        if (n < 0 || (size_t) n >= size)
                return -ENAMETOOLONG;
        return 0;
}

static uint64_t active_logical_size(JournalFile *file) {
        if (!file || !file->header)
                return 0;
        return le64toh(file->header->tail_object_offset);
}

static int close_archived_journal(MMapCache *cache, JournalFile *file) {
        int r;

        r = journal_file_archive(file, NULL);
        if (r < 0) {
                journal_file_close(file);
                mmap_cache_unref(cache);
                return r;
        }
        journal_file_offline_close(file);
        mmap_cache_unref(cache);
        return 0;
}

static int count_journal_files(const char *dir, uint64_t *ret_size, size_t *ret_count) {
        DIR *d;
        struct dirent *de;
        uint64_t total = 0;
        size_t count = 0;

        d = opendir(dir);
        if (!d)
                return -errno;

        while ((de = readdir(d))) {
                char path[PATH_MAX];
                struct stat st;
                int n;

                if (!endswith(de->d_name, ".journal"))
                        continue;
                n = snprintf(path, sizeof(path), "%s/%s", dir, de->d_name);
                if (n < 0 || (size_t) n >= sizeof(path)) {
                        closedir(d);
                        return -ENAMETOOLONG;
                }
                if (stat(path, &st) < 0) {
                        closedir(d);
                        return -errno;
                }
                total += (uint64_t) st.st_size;
                count++;
        }
        closedir(d);
        *ret_size = total;
        *ret_count = count;
        return 0;
}

static void print_json_string(const char *value) {
        const unsigned char *p;

        putchar('"');
        for (p = (const unsigned char *) value; p && *p; p++) {
                switch (*p) {
                case '"':
                        fputs("\\\"", stdout);
                        break;
                case '\\':
                        fputs("\\\\", stdout);
                        break;
                case '\b':
                        fputs("\\b", stdout);
                        break;
                case '\f':
                        fputs("\\f", stdout);
                        break;
                case '\n':
                        fputs("\\n", stdout);
                        break;
                case '\r':
                        fputs("\\r", stdout);
                        break;
                case '\t':
                        fputs("\\t", stdout);
                        break;
                default:
                        if (*p < 0x20)
                                printf("\\u%04x", *p);
                        else
                                putchar(*p);
                }
        }
        putchar('"');
}

static void print_journal_files_json(const char *dir) {
        DIR *d;
        struct dirent *de;
        bool first = true;

        putchar('[');
        d = opendir(dir);
        if (!d) {
                putchar(']');
                return;
        }

        while ((de = readdir(d))) {
                char path[PATH_MAX];
                int n;

                if (!endswith(de->d_name, ".journal"))
                        continue;
                n = snprintf(path, sizeof(path), "%s/%s", dir, de->d_name);
                if (n < 0 || (size_t) n >= sizeof(path))
                        continue;
                if (!first)
                        putchar(',');
                print_json_string(path);
                first = false;
        }
        closedir(d);
        putchar(']');
}

typedef struct DirectoryRun {
        MMapCache *cache;
        JournalFile *file;
        sd_id128_t boot_id;
        sd_id128_t seqnum_id;
        uint64_t seqnum;
        uint64_t journal_size;
        size_t records;
        size_t file_count;
        char journal_dir[PATH_MAX];
        char current_path[PATH_MAX];
        double append_seconds;
        double close_seconds;
} DirectoryRun;

static int setup_directory_run(DirectoryRun *run) {
        run->seqnum_id = SD_ID128_NULL;
        int r = id128_from_string("0123456789abcdef0123456789abcdef", &run->boot_id);
        if (r < 0)
                return r;
        r = mkdir_if_needed(arg_output);
        if (r < 0)
                return r;
        r = make_directory_path(run->journal_dir, sizeof(run->journal_dir));
        if (r < 0)
                return r;
        r = mkdir_if_needed(run->journal_dir);
        if (r < 0)
                return r;
        r = make_active_path(run->current_path, sizeof(run->current_path));
        if (r < 0)
                return r;
        return open_journal(run->current_path, arg_rotation_max_size, &run->cache, &run->file);
}

static int reopen_directory_journal(DirectoryRun *run) {
        (void) close_archived_journal(run->cache, run->file);
        run->cache = NULL;
        run->file = NULL;
        int r = make_active_path(run->current_path, sizeof(run->current_path));
        if (r < 0)
                return r;
        r = open_journal(run->current_path, arg_rotation_max_size, &run->cache, &run->file);
        if (r < 0)
                return r;
        return configure_header(run->file);
}

static int rotate_directory_if_needed(DirectoryRun *run) {
        if (run->records == 0 || active_logical_size(run->file) < arg_rotation_max_size)
                return 0;
        return reopen_directory_journal(run);
}

static int append_directory_entry(DirectoryRun *run, BenchRows *rows, size_t row) {
        struct dual_timestamp ts = {
                .realtime = base_realtime_usec + row * 500ULL,
                .monotonic = base_monotonic_usec + row * 50ULL,
        };
        return journal_file_append_entry(
                        run->file,
                        &ts,
                        &run->boot_id,
                        rows->iovecs + row * FIELDS_PER_ROW,
                        FIELDS_PER_ROW,
                        &run->seqnum,
                        &run->seqnum_id,
                        NULL,
                        NULL);
}

static int append_directory_row(DirectoryRun *run, BenchRows *rows, size_t row) {
        bool retried_after_rotation = false;
        for (;;) {
                int r = rotate_directory_if_needed(run);
                if (r < 0)
                        return r;
                r = append_directory_entry(run, rows, row);
                if (r >= 0) {
                        run->records++;
                        return 0;
                }
                if (run->records == 0 || retried_after_rotation)
                        return r;
                r = reopen_directory_journal(run);
                if (r < 0)
                        return r;
                retried_after_rotation = true;
        }
}

static int append_directory_rows(DirectoryRun *run, BenchRows *rows) {
        for (size_t row = 0; row < rows->rows; row++) {
                int r = append_directory_row(run, rows, row);
                if (r < 0)
                        return r;
        }
        return 0;
}

static int close_directory_run(DirectoryRun *run) {
        if (!run->file)
                return 0;
        int r = close_archived_journal(run->cache, run->file);
        run->cache = NULL;
        run->file = NULL;
        return r;
}

static int measure_directory_append(DirectoryRun *run, BenchRows *rows) {
        struct timespec start, end;
        (void) clock_gettime(CLOCK_MONOTONIC, &start);
        int r = append_directory_rows(run, rows);
        (void) clock_gettime(CLOCK_MONOTONIC, &end);
        run->append_seconds = elapsed_seconds(start, end);
        return r;
}

static int measure_directory_close(DirectoryRun *run) {
        struct timespec start, end;
        (void) clock_gettime(CLOCK_MONOTONIC, &start);
        int r = close_directory_run(run);
        (void) clock_gettime(CLOCK_MONOTONIC, &end);
        run->close_seconds = elapsed_seconds(start, end);
        return r;
}

static void print_directory_result(const DirectoryRun *run, double precompute_seconds, int status) {
        printf("{\"records\":%zu,\"fields_per_row\":%u,\"surface\":\"directory\",\"append_seconds\":%.9f,\"append_rows_per_second\":%.9f,\"close_seconds\":%.9f,\"total_writer_seconds\":%.9f,\"precompute_seconds\":%.9f,\"journal_size_bytes\":%" PRIu64 ",\"journal_path\":",
               run->records,
               FIELDS_PER_ROW,
               run->append_seconds,
               run->append_seconds > 0 ? (double) run->records / run->append_seconds : 0,
               run->close_seconds,
               run->append_seconds + run->close_seconds,
               precompute_seconds,
               run->journal_size);
        print_json_string(run->journal_dir);
        printf(",\"journal_directory\":");
        print_json_string(run->journal_dir);
        printf(",\"journal_files\":");
        print_journal_files_json(run->journal_dir);
        printf(",\"journal_file_count\":%zu,\"format\":\"%s\",\"compression\":\"none\",\"fss\":false,\"api_mode\":\"raw-payload\",\"live_publication\":\"systemd-default\",\"live_publish_every_entries\":1,\"data_hash_table_buckets\":%" PRIu64 ",\"field_hash_table_buckets\":1023,\"max_size_bytes\":%" PRIu64 ",\"rotation_max_size_bytes\":%" PRIu64 ",\"append_timer_excludes\":[\"row generation\",\"writer creation\",\"final close/sync\",\"journal verification\"],\"final_state\":\"archived\",\"errors\":%s}\n",
               run->file_count,
               arg_format,
               data_hash_buckets_for_max_size(arg_rotation_max_size),
               arg_rotation_max_size,
               arg_rotation_max_size,
               status < 0 ? "[\"failed\"]" : "[]");
}

static int run_directory_mode(BenchRows *rows, double precompute_seconds) {
        DirectoryRun run = {};
        int r = setup_directory_run(&run);
        if (r >= 0)
                r = measure_directory_append(&run, rows);
        int close_r = measure_directory_close(&run);
        if (r >= 0 && close_r < 0)
                r = close_r;
        if (r >= 0)
                r = count_journal_files(run.journal_dir, &run.journal_size, &run.file_count);
        print_directory_result(&run, precompute_seconds, r);
        if (run.file || run.cache)
                (void) close_archived_journal(run.cache, run.file);
        return r < 0 || run.records != rows->rows ? EXIT_FAILURE : EXIT_SUCCESS;
}

typedef struct DirectRun {
        MMapCache *cache;
        JournalFile *file;
        sd_id128_t boot_id;
        sd_id128_t seqnum_id;
        uint64_t seqnum;
        size_t records;
        double append_seconds;
        double close_seconds;
} DirectRun;

static void usage(const char *argv0) {
        fprintf(stderr, "usage: %s --output PATH [--rows N] [--format compact|regular] [--final-state online|offline|archived] [--max-size-bytes BYTES]\n", argv0);
}

static int build_bench_rows(BenchRows *rows, double *precompute_seconds) {
        struct timespec start, end;
        (void) clock_gettime(CLOCK_MONOTONIC, &start);
        int r = build_rows(arg_rows, rows);
        (void) clock_gettime(CLOCK_MONOTONIC, &end);
        *precompute_seconds = elapsed_seconds(start, end);
        return r;
}

static int setup_direct_run(DirectRun *run) {
        run->seqnum_id = SD_ID128_NULL;
        int r = id128_from_string("0123456789abcdef0123456789abcdef", &run->boot_id);
        if (r < 0)
                return r;
        return open_journal(arg_output, arg_max_size, &run->cache, &run->file);
}

static int append_direct_entry(DirectRun *run, BenchRows *rows, size_t row) {
        struct dual_timestamp ts = {
                .realtime = base_realtime_usec + row * 500ULL,
                .monotonic = base_monotonic_usec + row * 50ULL,
        };
        return journal_file_append_entry(
                        run->file,
                        &ts,
                        &run->boot_id,
                        rows->iovecs + row * FIELDS_PER_ROW,
                        FIELDS_PER_ROW,
                        &run->seqnum,
                        &run->seqnum_id,
                        NULL,
                        NULL);
}

static int append_direct_rows(DirectRun *run, BenchRows *rows) {
        for (size_t row = 0; row < rows->rows; row++) {
                int r = append_direct_entry(run, rows, row);
                if (r < 0)
                        return r;
                run->records++;
        }
        return 0;
}

static int measure_direct_append(DirectRun *run, BenchRows *rows) {
        struct timespec start, end;
        (void) clock_gettime(CLOCK_MONOTONIC, &start);
        int r = append_direct_rows(run, rows);
        (void) clock_gettime(CLOCK_MONOTONIC, &end);
        run->append_seconds = elapsed_seconds(start, end);
        return r;
}

static int measure_direct_close(DirectRun *run) {
        struct timespec start, end;
        (void) clock_gettime(CLOCK_MONOTONIC, &start);
        int r = close_journal(run->cache, run->file);
        run->cache = NULL;
        run->file = NULL;
        (void) clock_gettime(CLOCK_MONOTONIC, &end);
        run->close_seconds = elapsed_seconds(start, end);
        return r;
}

static void print_direct_result(const DirectRun *run, double precompute_seconds, int status) {
        printf("{\"records\":%zu,\"fields_per_row\":%u,\"append_seconds\":%.9f,\"append_rows_per_second\":%.9f,\"close_seconds\":%.9f,\"total_writer_seconds\":%.9f,\"precompute_seconds\":%.9f,\"journal_size_bytes\":%" PRIu64 ",\"journal_path\":\"%s\",\"format\":\"%s\",\"compression\":\"none\",\"fss\":false,\"api_mode\":\"raw-payload\",\"live_publication\":\"systemd-default\",\"live_publish_every_entries\":1,\"data_hash_table_buckets\":%" PRIu64 ",\"field_hash_table_buckets\":1023,\"max_size_bytes\":%" PRIu64 ",\"append_timer_excludes\":[\"row generation\",\"writer creation\",\"final close/sync\",\"journal verification\"],\"final_state\":\"%s\",\"errors\":%s}\n",
               run->records,
               FIELDS_PER_ROW,
               run->append_seconds,
               run->append_seconds > 0 ? (double) run->records / run->append_seconds : 0,
               run->close_seconds,
               run->append_seconds + run->close_seconds,
               precompute_seconds,
               file_size_or_zero(journal_path_after_close()),
               journal_path_after_close(),
               arg_format,
               data_hash_buckets_for_max_size(arg_max_size),
               arg_max_size,
               arg_final_state,
               status < 0 ? "[\"failed\"]" : "[]");
}

static int run_direct_mode(BenchRows *rows, double precompute_seconds) {
        DirectRun run = {};
        int r = setup_direct_run(&run);
        if (r >= 0)
                r = measure_direct_append(&run, rows);
        {
                int close_r = measure_direct_close(&run);
                if (r >= 0 && close_r < 0)
                        r = close_r;
        }
        print_direct_result(&run, precompute_seconds, r);
        if (run.file || run.cache)
                (void) close_journal(run.cache, run.file);
        return r < 0 || run.records != rows->rows ? EXIT_FAILURE : EXIT_SUCCESS;
}

static int run_selected_surface(BenchRows *rows, double precompute_seconds) {
        if (streq(arg_surface, "directory"))
                return run_directory_mode(rows, precompute_seconds);
        return run_direct_mode(rows, precompute_seconds);
}

int main(int argc, char **argv) {
        BenchRows rows = {};
        double precompute_seconds = 0;
        int r = parse_args(argc, argv);
        if (r < 0) {
                usage(argv[0]);
                return EXIT_FAILURE;
        }

        r = build_bench_rows(&rows, &precompute_seconds);
        if (r < 0) {
                DirectRun failed = {};
                print_direct_result(&failed, precompute_seconds, r);
        } else
                r = run_selected_surface(&rows, precompute_seconds);
        free_rows(&rows);
        return r < 0 ? EXIT_FAILURE : r;
}
