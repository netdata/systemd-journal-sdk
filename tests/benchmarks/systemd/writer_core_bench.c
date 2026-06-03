/* SPDX-License-Identifier: LGPL-2.1-or-later */

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

static int parse_args(int argc, char **argv) {
        for (int i = 1; i < argc; i++) {
                if (streq(argv[i], "--output") && i + 1 < argc)
                        arg_output = argv[++i];
                else if (streq(argv[i], "--rows") && i + 1 < argc) {
                        char *end = NULL;
                        unsigned long long value;

                        errno = 0;
                        value = strtoull(argv[++i], &end, 10);
                        if (errno != 0 || !end || *end != '\0' || value == 0)
                                return -EINVAL;
                        arg_rows = (size_t) value;
                } else if (streq(argv[i], "--format") && i + 1 < argc)
                        arg_format = argv[++i];
                else if (streq(argv[i], "--final-state") && i + 1 < argc)
                        arg_final_state = argv[++i];
                else if (streq(argv[i], "--max-size-bytes") && i + 1 < argc) {
                        char *end = NULL;
                        unsigned long long value;

                        errno = 0;
                        value = strtoull(argv[++i], &end, 10);
                        if (errno != 0 || !end || *end != '\0' || value == 0)
                                return -EINVAL;
                        arg_max_size = (uint64_t) value;
                } else if (streq(argv[i], "--rotation-max-size-bytes") && i + 1 < argc) {
                        char *end = NULL;
                        unsigned long long value;

                        errno = 0;
                        value = strtoull(argv[++i], &end, 10);
                        if (errno != 0 || !end || *end != '\0' || value == 0)
                                return -EINVAL;
                        arg_rotation_max_size = (uint64_t) value;
                } else if (streq(argv[i], "--surface") && i + 1 < argc)
                        arg_surface = argv[++i];
                else if (streq(argv[i], "--api-mode") && i + 1 < argc) {
                        if (!streq(argv[++i], "raw-payload"))
                                return -EINVAL;
                } else if (streq(argv[i], "--live-publish-every-entries") && i + 1 < argc) {
                        char *end = NULL;

                        errno = 0;
                        (void) strtoull(argv[++i], &end, 10);
                        if (errno != 0 || !end || *end != '\0')
                                return -EINVAL;
                } else
                        return -EINVAL;
        }

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

static int run_directory_mode(BenchRows *rows, double precompute_seconds) {
        MMapCache *cache = NULL;
        JournalFile *file = NULL;
        sd_id128_t boot_id, seqnum_id = SD_ID128_NULL;
        uint64_t seqnum = 0, journal_size = 0;
        size_t records = 0, file_count = 0;
        char journal_dir[PATH_MAX] = "", current_path[PATH_MAX] = "";
        struct timespec append_start, append_end, close_start, close_end;
        double append_seconds = 0, close_seconds = 0;
        int r;

        r = id128_from_string("0123456789abcdef0123456789abcdef", &boot_id);
        if (r < 0)
                goto finish;
        r = mkdir_if_needed(arg_output);
        if (r < 0)
                goto finish;
        r = make_directory_path(journal_dir, sizeof(journal_dir));
        if (r < 0)
                goto finish;
        r = mkdir_if_needed(journal_dir);
        if (r < 0)
                goto finish;
        r = make_active_path(current_path, sizeof(current_path));
        if (r < 0)
                goto finish;
        r = open_journal(current_path, arg_rotation_max_size, &cache, &file);
        if (r < 0)
                goto finish;

        (void) clock_gettime(CLOCK_MONOTONIC, &append_start);
        for (size_t row = 0; row < rows->rows; row++) {
                struct dual_timestamp ts = {
                        .realtime = base_realtime_usec + row * 500ULL,
                        .monotonic = base_monotonic_usec + row * 50ULL,
                };
                bool retried_after_rotation = false;

                if (records > 0 && active_logical_size(file) >= arg_rotation_max_size) {
                        int close_r;

                        (void) close_archived_journal(cache, file);
                        cache = NULL;
                        file = NULL;
                        r = make_active_path(current_path, sizeof(current_path));
                        if (r < 0)
                                break;
                        r = open_journal(current_path, arg_rotation_max_size, &cache, &file);
                        if (r < 0)
                                break;
                        close_r = configure_header(file);
                        if (close_r < 0) {
                                r = close_r;
                                break;
                        }
                }

retry_append:
                r = journal_file_append_entry(
                                file,
                                &ts,
                                &boot_id,
                                rows->iovecs + row * FIELDS_PER_ROW,
                                FIELDS_PER_ROW,
                                &seqnum,
                                &seqnum_id,
                                NULL,
                                NULL);
                if (r < 0 && records > 0 && !retried_after_rotation) {
                        int close_r;

                        (void) close_archived_journal(cache, file);
                        cache = NULL;
                        file = NULL;
                        r = make_active_path(current_path, sizeof(current_path));
                        if (r < 0)
                                break;
                        r = open_journal(current_path, arg_rotation_max_size, &cache, &file);
                        if (r < 0)
                                break;
                        close_r = configure_header(file);
                        if (close_r < 0) {
                                r = close_r;
                                break;
                        }
                        retried_after_rotation = true;
                        goto retry_append;
                }
                if (r < 0)
                        break;
                records++;
        }
        (void) clock_gettime(CLOCK_MONOTONIC, &append_end);
        append_seconds = elapsed_seconds(append_start, append_end);

        (void) clock_gettime(CLOCK_MONOTONIC, &close_start);
        if (file) {
                int close_r = close_archived_journal(cache, file);
                cache = NULL;
                file = NULL;
                if (r >= 0 && close_r < 0)
                        r = close_r;
        }
        (void) clock_gettime(CLOCK_MONOTONIC, &close_end);
        close_seconds = elapsed_seconds(close_start, close_end);

        if (r >= 0)
                r = count_journal_files(journal_dir, &journal_size, &file_count);

finish:
        printf("{\"records\":%zu,\"fields_per_row\":%u,\"surface\":\"directory\",\"append_seconds\":%.9f,\"append_rows_per_second\":%.9f,\"close_seconds\":%.9f,\"total_writer_seconds\":%.9f,\"precompute_seconds\":%.9f,\"journal_size_bytes\":%" PRIu64 ",\"journal_path\":",
               records,
               FIELDS_PER_ROW,
               append_seconds,
               append_seconds > 0 ? (double) records / append_seconds : 0,
               close_seconds,
               append_seconds + close_seconds,
               precompute_seconds,
               journal_size);
        print_json_string(journal_dir);
        printf(",\"journal_directory\":");
        print_json_string(journal_dir);
        printf(",\"journal_files\":");
        print_journal_files_json(journal_dir);
        printf(",\"journal_file_count\":%zu,\"format\":\"%s\",\"compression\":\"none\",\"fss\":false,\"api_mode\":\"raw-payload\",\"live_publication\":\"systemd-default\",\"live_publish_every_entries\":1,\"data_hash_table_buckets\":%" PRIu64 ",\"field_hash_table_buckets\":1023,\"max_size_bytes\":%" PRIu64 ",\"rotation_max_size_bytes\":%" PRIu64 ",\"append_timer_excludes\":[\"row generation\",\"writer creation\",\"final close/sync\",\"journal verification\"],\"final_state\":\"archived\",\"errors\":%s}\n",
               file_count,
               arg_format,
               data_hash_buckets_for_max_size(arg_rotation_max_size),
               arg_rotation_max_size,
               arg_rotation_max_size,
               r < 0 ? "[\"failed\"]" : "[]");

        if (file || cache)
                (void) close_archived_journal(cache, file);
        return r < 0 || records != rows->rows ? EXIT_FAILURE : EXIT_SUCCESS;
}

int main(int argc, char **argv) {
        BenchRows rows = {};
        MMapCache *cache = NULL;
        JournalFile *file = NULL;
        sd_id128_t boot_id, seqnum_id = SD_ID128_NULL;
        uint64_t seqnum = 0;
        size_t records = 0;
        struct timespec precompute_start, precompute_end, append_start, append_end, close_start, close_end;
        double precompute_seconds = 0, append_seconds = 0, close_seconds = 0;
        int r;

        r = parse_args(argc, argv);
        if (r < 0) {
                fprintf(stderr, "usage: %s --output PATH [--rows N] [--format compact|regular] [--final-state online|offline|archived] [--max-size-bytes BYTES]\n", argv[0]);
                return EXIT_FAILURE;
        }

        r = id128_from_string("0123456789abcdef0123456789abcdef", &boot_id);
        if (r < 0)
                goto finish;

        (void) clock_gettime(CLOCK_MONOTONIC, &precompute_start);
        r = build_rows(arg_rows, &rows);
        (void) clock_gettime(CLOCK_MONOTONIC, &precompute_end);
        precompute_seconds = elapsed_seconds(precompute_start, precompute_end);
        if (r < 0)
                goto finish;

        if (streq(arg_surface, "directory")) {
                int ret = run_directory_mode(&rows, precompute_seconds);
                free_rows(&rows);
                return ret;
        }

        r = open_journal(arg_output, arg_max_size, &cache, &file);
        if (r < 0)
                goto finish;

        (void) clock_gettime(CLOCK_MONOTONIC, &append_start);
        for (size_t row = 0; row < rows.rows; row++) {
                struct dual_timestamp ts = {
                        .realtime = base_realtime_usec + row * 500ULL,
                        .monotonic = base_monotonic_usec + row * 50ULL,
                };

                r = journal_file_append_entry(
                                file,
                                &ts,
                                &boot_id,
                                rows.iovecs + row * FIELDS_PER_ROW,
                                FIELDS_PER_ROW,
                                &seqnum,
                                &seqnum_id,
                                NULL,
                                NULL);
                if (r < 0)
                        break;
                records++;
        }
        (void) clock_gettime(CLOCK_MONOTONIC, &append_end);
        append_seconds = elapsed_seconds(append_start, append_end);

        (void) clock_gettime(CLOCK_MONOTONIC, &close_start);
        {
                int close_r = close_journal(cache, file);
                cache = NULL;
                file = NULL;
                if (r >= 0 && close_r < 0)
                        r = close_r;
        }
        (void) clock_gettime(CLOCK_MONOTONIC, &close_end);
        close_seconds = elapsed_seconds(close_start, close_end);

finish:
        printf("{\"records\":%zu,\"fields_per_row\":%u,\"append_seconds\":%.9f,\"append_rows_per_second\":%.9f,\"close_seconds\":%.9f,\"total_writer_seconds\":%.9f,\"precompute_seconds\":%.9f,\"journal_size_bytes\":%" PRIu64 ",\"journal_path\":\"%s\",\"format\":\"%s\",\"compression\":\"none\",\"fss\":false,\"api_mode\":\"raw-payload\",\"live_publication\":\"systemd-default\",\"live_publish_every_entries\":1,\"data_hash_table_buckets\":%" PRIu64 ",\"field_hash_table_buckets\":1023,\"max_size_bytes\":%" PRIu64 ",\"append_timer_excludes\":[\"row generation\",\"writer creation\",\"final close/sync\",\"journal verification\"],\"final_state\":\"%s\",\"errors\":%s}\n",
               records,
               FIELDS_PER_ROW,
               append_seconds,
               append_seconds > 0 ? (double) records / append_seconds : 0,
               close_seconds,
               append_seconds + close_seconds,
               precompute_seconds,
               file_size_or_zero(journal_path_after_close()),
               journal_path_after_close(),
               arg_format,
               data_hash_buckets_for_max_size(arg_max_size),
               arg_max_size,
               arg_final_state,
               r < 0 ? "[\"failed\"]" : "[]");

        if (file || cache)
                (void) close_journal(cache, file);
        free_rows(&rows);
        return r < 0 || records != arg_rows ? EXIT_FAILURE : EXIT_SUCCESS;
}
