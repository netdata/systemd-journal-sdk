/* SPDX-License-Identifier: LGPL-2.1-or-later */

// cppcheck-suppress-file missingIncludeSystem
#include <errno.h>
#include <assert.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/uio.h>
#include <unistd.h>

#include "sd-id128.h"

#if __has_include("sd-json.h")
#include "sd-json.h"
#define MATRIX_FSPRG_RETURNS_INT 1
#define MatrixJsonVariant sd_json_variant
#define matrix_json_parse sd_json_parse
#define matrix_json_variant_unref sd_json_variant_unref
#define matrix_json_variant_by_key sd_json_variant_by_key
#define matrix_json_variant_by_index sd_json_variant_by_index
#define matrix_json_variant_is_string sd_json_variant_is_string
#define matrix_json_variant_is_unsigned sd_json_variant_is_unsigned
#define matrix_json_variant_is_array sd_json_variant_is_array
#define matrix_json_variant_is_object sd_json_variant_is_object
#define matrix_json_variant_is_null sd_json_variant_is_null
#define matrix_json_variant_string sd_json_variant_string
#define matrix_json_variant_unsigned sd_json_variant_unsigned
#define matrix_json_variant_elements sd_json_variant_elements
#define matrix_json_variant_unbase64 sd_json_variant_unbase64
#else
#include "json.h"
#define MATRIX_FSPRG_RETURNS_INT 0
#define MatrixJsonVariant JsonVariant
#define matrix_json_parse json_parse
#define matrix_json_variant_unref json_variant_unref
#define matrix_json_variant_by_key json_variant_by_key
#define matrix_json_variant_by_index json_variant_by_index
#define matrix_json_variant_is_string json_variant_is_string
#define matrix_json_variant_is_unsigned json_variant_is_unsigned
#define matrix_json_variant_is_array json_variant_is_array
#define matrix_json_variant_is_object json_variant_is_object
#define matrix_json_variant_is_null json_variant_is_null
#define matrix_json_variant_string json_variant_string
#define matrix_json_variant_unsigned json_variant_unsigned
#define matrix_json_variant_elements json_variant_elements
#define matrix_json_variant_unbase64 json_variant_unbase64
#endif

#include "alloc-util.h"
#include "fsprg.h"
#include "journal-file.h"
#if __has_include("journal-file-util.h")
#include "journal-file-util.h"
#define MATRIX_HAS_JOURNAL_FILE_UTIL 1
#else
#define MATRIX_HAS_JOURNAL_FILE_UTIL 0
#endif
#include "journal-def.h"
#if __has_include("iovec-util.h")
#include "iovec-util.h"
#elif __has_include("io-util.h")
#include "io-util.h"
#endif
#include "log.h"
#include "mmap-cache.h"
#include "string-util.h"
#include "time-util.h"

static const char *arg_dataset = NULL;
static const char *arg_output = NULL;
static const char *arg_fss_root = NULL;
static bool arg_rejection_mode = false;
static bool arg_compact = false;
static bool arg_sealed = false;
static uint64_t arg_max_size = 64ULL * 1024ULL * 1024ULL;
static enum {
        FINAL_STATE_ONLINE,
        FINAL_STATE_OFFLINE,
        FINAL_STATE_ARCHIVED,
} arg_final_state = FINAL_STATE_ONLINE;

static size_t cstring_len(const char *s) {
        size_t n = 0;

        while (s[n] != '\0')
                n++;
        return n;
}

static void usage(const char *argv0) {
        fprintf(stderr, "usage: %s --dataset PATH --output PATH [--rejection-mode] [--final-state online|offline|archived] [--compact] [--sealed --fss-root PATH] [--max-size-bytes BYTES]\n", argv0);
}

static int parse_final_state(const char *state) {
        if (streq(state, "online")) {
                arg_final_state = FINAL_STATE_ONLINE;
                return 0;
        }
        if (streq(state, "offline")) {
                arg_final_state = FINAL_STATE_OFFLINE;
                return 0;
        }
        if (streq(state, "archived")) {
                arg_final_state = FINAL_STATE_ARCHIVED;
                return 0;
        }
        fprintf(stderr, "invalid final state: %s\n", state);
        return -EINVAL;
}

static int parse_max_size(const char *text) {
        char *end = NULL;
        errno = 0;
        unsigned long long value = strtoull(text, &end, 10);
        if (errno != 0 || !end || *end != '\0' || value == 0) {
                fprintf(stderr, "invalid max size: %s\n", text);
                return -EINVAL;
        }
        arg_max_size = (uint64_t) value;
        return 0;
}

static int parse_string_arg(const char *arg, const char *value) {
        if (streq(arg, "--dataset")) {
                arg_dataset = value;
                return 1;
        }
        if (streq(arg, "--output")) {
                arg_output = value;
                return 1;
        }
        if (streq(arg, "--fss-root")) {
                arg_fss_root = value;
                return 1;
        }
        return 0;
}

static int parse_flag_arg(const char *arg) {
        if (streq(arg, "--rejection-mode")) {
                arg_rejection_mode = true;
                return 1;
        }
        if (streq(arg, "--compact")) {
                arg_compact = true;
                return 1;
        }
        if (streq(arg, "--sealed")) {
                arg_sealed = true;
                return 1;
        }
        return 0;
}

static int parse_value_arg(int argc, char **argv, int *idx) {
        const char *arg = argv[*idx];
        if (*idx + 1 >= argc)
                return -EINVAL;
        const char *value = argv[++(*idx)];
        if (parse_string_arg(arg, value))
                return 0;
        if (streq(arg, "--final-state"))
                return parse_final_state(value);
        if (streq(arg, "--max-size-bytes"))
                return parse_max_size(value);
        return -EINVAL;
}

static int parse_one_arg(int argc, char **argv, int *idx) {
        if (parse_flag_arg(argv[*idx]))
                return 0;
        return parse_value_arg(argc, argv, idx);
}

static int validate_args(void) {
        if (!arg_dataset || !arg_output) {
                return -EINVAL;
        }
        if (arg_sealed && !arg_fss_root) {
                fprintf(stderr, "--sealed requires --fss-root\n");
                return -EINVAL;
        }
        return 0;
}

static int parse_args(int argc, char **argv) {
        for (int i = 1; i < argc; i++) {
                int r = parse_one_arg(argc, argv, &i);
                if (r < 0) {
                        usage(argv[0]);
                        return r;
                }
        }
        int r = validate_args();
        if (r < 0)
                usage(argv[0]);
        return r;
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
#if defined(PROJECT_VERSION) && PROJECT_VERSION >= 254
        f->header->tail_entry_boot_id = boot_id;
#else
        (void) boot_id;
#endif
        return 0;
}

static int mkdir_p_simple(const char *path) {
        char *copy;
        int r = 0;

        copy = strdup(path);
        if (!copy)
                return -ENOMEM;

        for (char *p = copy + 1; *p; p++) {
                if (*p != '/')
                        continue;
                *p = '\0';
                if (mkdir(copy, 0700) < 0 && errno != EEXIST) {
                        r = -errno;
                        goto finish;
                }
                *p = '/';
        }
        if (mkdir(copy, 0700) < 0 && errno != EEXIST)
                r = -errno;

finish:
        free(copy);
        return r;
}

static int write_all_fd(int fd, const void *data, size_t size) {
        const uint8_t *p = data;

        while (size > 0) {
                ssize_t n = write(fd, p, size);
                if (n < 0)
                        return -errno;
                if (n == 0)
                        return -EIO;
                p += n;
                size -= (size_t) n;
        }
        return 0;
}

static int format_verification_key(
                const uint8_t *seed,
                size_t seed_size,
                uint64_t start,
                uint64_t interval,
                char **ret) {

        FILE *f;
        char *buffer = NULL;
        size_t size = 0;

        assert(seed);
        assert(ret);

        f = open_memstream(&buffer, &size);
        if (!f)
                return -ENOMEM;

        for (size_t i = 0; i < seed_size; i++) {
                if (i > 0 && i % 3 == 0)
                        fputc('-', f);
                fprintf(f, "%02x", seed[i]);
        }

        fprintf(f, "/%"PRIx64"-%"PRIx64, start, interval);

        if (fclose(f) != 0) {
                free(buffer);
                return -errno;
        }

        *ret = buffer;
        return 0;
}

#if HAVE_GCRYPT
static int synthetic_fss_ids(sd_id128_t *machine, sd_id128_t *boot) {
        int r = sd_id128_get_machine(machine);
        if (r < 0)
                return r;
        return id128_from_string("0123456789abcdef0123456789abcdef", boot);
}

static int make_synthetic_fss_paths(char **ret_machine_path, char **ret_fss_path, sd_id128_t machine) {
        char *machine_path = NULL;
        char *fss_path = NULL;

        if (asprintf(&machine_path, "%s/" SD_ID128_FORMAT_STR,
                     arg_fss_root, SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
        if (asprintf(&fss_path, "%s/fss", machine_path) < 0) {
                free(machine_path);
                return -ENOMEM;
        }

        *ret_machine_path = machine_path;
        *ret_fss_path = fss_path;
        return 0;
}

static void fill_synthetic_seed(uint8_t *seed, size_t seed_size) {
        for (size_t i = 0; i < seed_size; i++)
                seed[i] = (uint8_t) (3 + i * 17);
}

static int generate_synthetic_fss_state(
                uint8_t *mpk,
                uint8_t *seed,
                size_t seed_size,
                uint8_t *state) {

#if MATRIX_FSPRG_RETURNS_INT
        int r = FSPRG_GenMK(NULL, mpk, seed, seed_size, FSPRG_RECOMMENDED_SECPAR);
        if (r < 0)
                return r;
        return FSPRG_GenState0(state, mpk, seed, seed_size);
#else
        FSPRG_GenMK(NULL, mpk, seed, seed_size, FSPRG_RECOMMENDED_SECPAR);
        FSPRG_GenState0(state, mpk, seed, seed_size);
        return 0;
#endif
}

static struct FSSHeader synthetic_fss_header(
                sd_id128_t machine,
                sd_id128_t boot,
                uint64_t start_usec,
                uint64_t interval,
                size_t state_size) {

        return (struct FSSHeader) {
                .signature = { 'K', 'S', 'H', 'H', 'R', 'H', 'L', 'P' },
                .machine_id = machine,
                .boot_id = boot,
                .header_size = htole64(sizeof(struct FSSHeader)),
                .start_usec = htole64(start_usec),
                .interval_usec = htole64(interval),
                .fsprg_secpar = htole16(FSPRG_RECOMMENDED_SECPAR),
                .fsprg_state_size = htole64(state_size),
        };
}

static int write_synthetic_fss_file(
                const char *fss_path,
                const struct FSSHeader *header,
                const uint8_t *state,
                size_t state_size) {

        int fd = open(fss_path, O_WRONLY|O_CREAT|O_TRUNC|O_CLOEXEC, 0600);
        if (fd < 0)
                return -errno;

        int r = write_all_fd(fd, header, sizeof(*header));
        if (r >= 0)
                r = write_all_fd(fd, state, state_size);
        if (r >= 0 && fsync(fd) < 0)
                r = -errno;
        if (close(fd) < 0 && r >= 0)
                r = -errno;
        return r;
}
#endif

static int setup_synthetic_fss(char **ret_verification_key) {
#if HAVE_GCRYPT
        const uint64_t interval = 15ULL * 60ULL * 1000ULL * 1000ULL;
        const uint64_t start = 1699999200000000ULL / interval;
        _cleanup_free_ char *machine_path = NULL;
        _cleanup_free_ char *fss_path = NULL;
        sd_id128_t machine, boot;
        size_t mpk_size, seed_size, state_size;
        uint8_t *mpk, *seed, *state;
        struct FSSHeader h;
        int r;

        assert(ret_verification_key);

        r = synthetic_fss_ids(&machine, &boot);
        if (r < 0)
                return r;
        r = make_synthetic_fss_paths(&machine_path, &fss_path, machine);
        if (r < 0)
                return r;
        r = mkdir_p_simple(machine_path);
        if (r < 0)
                return r;

        mpk_size = FSPRG_mskinbytes(FSPRG_RECOMMENDED_SECPAR);
        mpk = alloca_safe(mpk_size);

        seed_size = FSPRG_RECOMMENDED_SEEDLEN;
        seed = alloca_safe(seed_size);
        fill_synthetic_seed(seed, seed_size);

        state_size = FSPRG_stateinbytes(FSPRG_RECOMMENDED_SECPAR);
        state = alloca_safe(state_size);

#if MATRIX_FSPRG_RETURNS_INT
        r = generate_synthetic_fss_state(mpk, seed, seed_size, state);
        if (r < 0)
                return r;
#else
        (void) generate_synthetic_fss_state(mpk, seed, seed_size, state);
#endif

        h = synthetic_fss_header(machine, boot, start * interval, interval, state_size);
        r = write_synthetic_fss_file(fss_path, &h, state, state_size);
        if (r < 0)
                return r;

        return format_verification_key(seed, seed_size, start, interval, ret_verification_key);
#else
        return -EOPNOTSUPP;
#endif
}

static int open_journal(const char *path, uint64_t max_size, MMapCache **ret_cache, JournalFile **ret_file) {
        JournalMetrics metrics;
        MMapCache *cache;
        JournalFile *file = NULL;
        int r;

        assert(ret_cache);
        assert(ret_file);

        (void) setenv("SYSTEMD_JOURNAL_COMPRESS", "0", 1);
        (void) setenv("SYSTEMD_JOURNAL_COMPACT", arg_compact ? "1" : "0", 1);
        (void) setenv("SYSTEMD_JOURNAL_KEYED_HASH", "1", 1);
        if (arg_sealed)
                (void) setenv("SYSTEMD_JOURNAL_FSS_ROOT", arg_fss_root, 1);

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
                        arg_sealed ? JOURNAL_SEAL : 0,
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

        if (!arg_sealed) {
                r = configure_header(file);
                if (r < 0) {
                        journal_file_close(file);
                        mmap_cache_unref(cache);
                        return r;
                }
        }

        *ret_cache = cache;
        *ret_file = file;
        return 0;
}

static int close_journal(MMapCache *cache, JournalFile *file) {
        int r = 0;

        if (file) {
                if (arg_final_state == FINAL_STATE_ARCHIVED) {
                        r = journal_file_archive(file, NULL);
                        if (r < 0)
                                journal_file_close(file);
#if MATRIX_HAS_JOURNAL_FILE_UTIL
                        else
                                journal_file_offline_close(file);
#else
                        else {
                                (void) journal_file_set_offline_thread_join(file);
                                journal_file_close(file);
                        }
#endif
#if MATRIX_HAS_JOURNAL_FILE_UTIL
                } else if (arg_final_state == FINAL_STATE_OFFLINE)
                        journal_file_offline_close(file);
#else
                } else if (arg_final_state == FINAL_STATE_OFFLINE) {
                        (void) journal_file_set_offline_thread_join(file);
                        journal_file_close(file);
                }
#endif
                else {
                        (void) journal_file_set_offline_thread_join(file);
                        journal_file_close(file);
                }
        }
        if (cache)
                mmap_cache_unref(cache);
        return r;
}

static MatrixJsonVariant *by_key(MatrixJsonVariant *v, const char *key) {
        return matrix_json_variant_by_key(v, key);
}

static const char *string_by_key(MatrixJsonVariant *v, const char *key) {
        MatrixJsonVariant *child = by_key(v, key);

        if (!child || !matrix_json_variant_is_string(child))
                return NULL;
        return matrix_json_variant_string(child);
}

static int unsigned_by_key(MatrixJsonVariant *v, const char *key, uint64_t *ret) {
        MatrixJsonVariant *child = by_key(v, key);

        if (!child || !matrix_json_variant_is_unsigned(child))
                return -EINVAL;
        *ret = matrix_json_variant_unsigned(child);
        return 0;
}

static int materialize_utf8_value(MatrixJsonVariant *value, void **ret, size_t *ret_size) {
        const char *text = string_by_key(value, "text");
        if (!text)
                return -EINVAL;
        char *copy = strdup(text);
        if (!copy)
                return -ENOMEM;
        *ret = copy;
        *ret_size = cstring_len(text);
        return 0;
}

static int materialize_bytes_value(MatrixJsonVariant *value, void **ret, size_t *ret_size) {
        MatrixJsonVariant *base64 = by_key(value, "base64");
        uint64_t expected = 0;
        if (!base64 || !matrix_json_variant_is_string(base64))
                return -EINVAL;
        int r = matrix_json_variant_unbase64(base64, ret, ret_size);
        if (r < 0)
                return r;
        r = unsigned_by_key(value, "size", &expected);
        if (r >= 0 && *ret_size != expected) {
                free(*ret);
                *ret = NULL;
                return -EBADMSG;
        }
        return 0;
}

static int materialize_repeat_value(MatrixJsonVariant *value, void **ret, size_t *ret_size) {
        uint64_t byte, size;
        int r = unsigned_by_key(value, "byte", &byte);
        if (r < 0)
                return r;
        r = unsigned_by_key(value, "size", &size);
        if (r < 0)
                return r;
        if (byte > 255 || size > SIZE_MAX)
                return -EINVAL;
        void *p = malloc((size_t) size);
        if (!p && size > 0)
                return -ENOMEM;
        memset(p, (uint8_t) byte, (size_t) size);
        *ret = p;
        *ret_size = (size_t) size;
        return 0;
}

static int materialize_value(MatrixJsonVariant *value, void **ret, size_t *ret_size) {
        assert(value);
        assert(ret);
        assert(ret_size);
        const char *kind = string_by_key(value, "kind");
        if (!kind)
                return -EINVAL;
        if (streq(kind, "utf8"))
                return materialize_utf8_value(value, ret, ret_size);
        if (streq(kind, "bytes"))
                return materialize_bytes_value(value, ret, ret_size);
        if (streq(kind, "repeat"))
                return materialize_repeat_value(value, ret, ret_size);
        return -EINVAL;
}

static int make_payload(const char *name, const void *value, size_t value_size, struct iovec *ret) {
        size_t name_size, payload_size;
        uint8_t *payload;

        assert(name);
        assert(ret);

        name_size = cstring_len(name);
        if (name_size > SIZE_MAX - 1 || name_size + 1 > SIZE_MAX - value_size)
                return -EOVERFLOW;

        payload_size = name_size + 1 + value_size;
        payload = malloc(payload_size);
        if (!payload)
                return -ENOMEM;

        memcpy(payload, name, name_size);
        payload[name_size] = '=';
        if (value_size > 0)
                memcpy(payload + name_size + 1, value, value_size);

        *ret = (struct iovec) {
                .iov_base = payload,
                .iov_len = payload_size,
        };
        return 0;
}

static int matrix_append_entry(
                JournalFile *file,
                const struct dual_timestamp *ts,
                const sd_id128_t *boot_id,
                struct iovec *iov,
                size_t n_iov,
                uint64_t *seqnum,
                sd_id128_t *seqnum_id) {

#if defined(PROJECT_VERSION) && PROJECT_VERSION >= 254
        return journal_file_append_entry(file, ts, boot_id, iov, n_iov, seqnum, seqnum_id, NULL, NULL);
#else
        (void) seqnum_id;
        if (n_iov > UINT_MAX)
                return -E2BIG;
        return journal_file_append_entry(file, ts, boot_id, iov, (unsigned) n_iov, seqnum, NULL, NULL);
#endif
}

static void free_iovecs(struct iovec *iov, size_t n) {
        if (!iov)
                return;
        for (size_t i = 0; i < n; i++)
                free(iov[i].iov_base);
        free(iov);
}

static int accepted_record_fields(MatrixJsonVariant *record, MatrixJsonVariant **ret_fields, size_t *ret_n_fields) {
        MatrixJsonVariant *fields = by_key(record, "fields");

        if (!fields || !matrix_json_variant_is_array(fields))
                return -EINVAL;

        size_t n_fields = matrix_json_variant_elements(fields);
        if (n_fields == 0)
                return -EINVAL;

        *ret_fields = fields;
        *ret_n_fields = n_fields;
        return 0;
}

static int build_field_iovec(MatrixJsonVariant *field, struct iovec *ret) {
        const char *name = string_by_key(field, "name");
        MatrixJsonVariant *value = by_key(field, "value");
        void *value_data = NULL;
        size_t value_size = 0;
        int r;

        if (!name || !value)
                return -EINVAL;

        r = materialize_value(value, &value_data, &value_size);
        if (r < 0)
                return r;

        r = make_payload(name, value_data, value_size, ret);
        free(value_data);
        return r;
}

static int accepted_record_timestamp(MatrixJsonVariant *record, struct dual_timestamp *ret) {
        int r = unsigned_by_key(record, "realtime_usec", &ret->realtime);
        if (r < 0)
                return r;
        return unsigned_by_key(record, "monotonic_usec", &ret->monotonic);
}

static int accepted_record_boot_id(sd_id128_t *ret) {
        return id128_from_string("0123456789abcdef0123456789abcdef", ret);
}

static int build_accepted_iovecs(MatrixJsonVariant *fields, struct iovec *iov, size_t n_fields) {
        for (size_t i = 0; i < n_fields; i++) {
                MatrixJsonVariant *field = matrix_json_variant_by_index(fields, i);
                int r = build_field_iovec(field, &iov[i]);
                if (r < 0)
                        return r;
        }
        return 0;
}

static int append_accepted_record(JournalFile *file, MatrixJsonVariant *record, uint64_t *seqnum, sd_id128_t *seqnum_id) {
        MatrixJsonVariant *fields = NULL;
        struct iovec *iov = NULL;
        size_t n_fields = 0;
        struct dual_timestamp ts;
        sd_id128_t boot_id;
        int r;

        r = accepted_record_fields(record, &fields, &n_fields);
        if (r < 0)
                return r;

        iov = calloc(n_fields, sizeof(struct iovec));
        if (!iov)
                return -ENOMEM;

        r = build_accepted_iovecs(fields, iov, n_fields);
        if (r < 0)
                goto finish;

        r = accepted_record_timestamp(record, &ts);
        if (r < 0)
                goto finish;

        r = accepted_record_boot_id(&boot_id);
        if (r < 0)
                goto finish;

        r = matrix_append_entry(file, &ts, &boot_id, iov, n_fields, seqnum, seqnum_id);

finish:
        free_iovecs(iov, n_fields);
        return r;
}

static int append_payload(JournalFile *file, const void *payload, size_t payload_size, uint64_t *seqnum, sd_id128_t *seqnum_id) {
        struct dual_timestamp ts = {
                .realtime = 1700000000000000ULL,
                .monotonic = 50000000ULL,
        };
        sd_id128_t boot_id;
        struct iovec iov;
        int r;

        r = id128_from_string("0123456789abcdef0123456789abcdef", &boot_id);
        if (r < 0)
                return r;

        iov = IOVEC_MAKE((void*) payload, payload_size);
        return matrix_append_entry(file, &ts, &boot_id, &iov, 1, seqnum, seqnum_id);
}

static int append_raw_payload(JournalFile *file, const char *payload, uint64_t *seqnum, sd_id128_t *seqnum_id) {
        return append_payload(file, payload, cstring_len(payload), seqnum, seqnum_id);
}

static int append_field_payload(JournalFile *file, const char *name, MatrixJsonVariant *value, uint64_t *seqnum, sd_id128_t *seqnum_id) {
        struct iovec iov = {};
        void *value_data = NULL;
        size_t value_size = 0;
        int r;

        r = materialize_value(value, &value_data, &value_size);
        if (r < 0)
                return r;

        r = make_payload(name, value_data, value_size, &iov);
        free(value_data);
        if (r < 0)
                return r;

        r = append_payload(file, iov.iov_base, iov.iov_len, seqnum, seqnum_id);
        free(iov.iov_base);
        return r;
}

static const char *classify_error(int r) {
        if (r == -E2BIG)
                return "E2BIG";
        return "EINVAL";
}

typedef struct AcceptedRun {
        MMapCache *cache;
        JournalFile *file;
        FILE *input;
        char *line;
        char *verification_key;
        size_t line_alloc;
        size_t records;
        size_t errors;
        uint64_t seqnum;
        sd_id128_t seqnum_id;
} AcceptedRun;

static void accepted_run_free(AcceptedRun *run) {
        free(run->line);
        if (run->input)
                fclose(run->input);
        free(run->verification_key);
}

static int accepted_run_close_journal(AcceptedRun *run, int status) {
        int close_r = close_journal(run->cache, run->file);
        run->cache = NULL;
        run->file = NULL;
        if (status >= 0 && close_r < 0)
                return close_r;
        return status;
}

static int accepted_run_open(AcceptedRun *run) {
        int r;

        if (arg_sealed) {
                r = setup_synthetic_fss(&run->verification_key);
                if (r < 0) {
                        fprintf(stderr, "setup synthetic FSS failed: %s\n", strerror(-r));
                        return r;
                }
        }

        r = open_journal(arg_output, arg_max_size, &run->cache, &run->file);
        if (r < 0) {
                fprintf(stderr, "open journal failed: %s\n", strerror(-r));
                return r;
        }

        run->input = fopen(arg_dataset, "re");
        return run->input ? 0 : -errno;
}

static bool is_record_type(MatrixJsonVariant *record, const char *expected) {
        const char *record_type = string_by_key(record, "record_type");
        return record_type && streq(record_type, expected);
}

static void accepted_run_append_record(AcceptedRun *run, MatrixJsonVariant *record) {
        int r = append_accepted_record(run->file, record, &run->seqnum, &run->seqnum_id);

        if (r < 0) {
                const char *entry_id = string_by_key(record, "entry_id");
                fprintf(stderr, "%s: append failed: %s\n", entry_id ?: "record", strerror(-r));
                run->errors++;
                return;
        }
        run->records++;
}

static void accepted_run_process_line(AcceptedRun *run, const char *line) {
        MatrixJsonVariant *record = NULL;
        int r = matrix_json_parse(line, 0, &record, NULL, NULL);

        if (r < 0) {
                run->errors++;
                return;
        }
        if (is_record_type(record, "accepted"))
                accepted_run_append_record(run, record);
        matrix_json_variant_unref(record);
}

static void accepted_run_process_dataset(AcceptedRun *run) {
        while (getline(&run->line, &run->line_alloc, run->input) >= 0)
                accepted_run_process_line(run, run->line);
}

static void accepted_run_print_result(const AcceptedRun *run, int status) {
        const char *sealed = arg_sealed ? "true" : "false";

        if (status < 0)
                printf("{\"records\":%zu,\"sealed\":%s,\"errors\":[\"failed\"]}\n",
                       run->records, sealed);
        else if (run->verification_key)
                printf("{\"records\":%zu,\"sealed\":true,\"verification_key\":\"%s\",\"errors\":[]}\n",
                       run->records, run->verification_key);
        else
                printf("{\"records\":%zu,\"sealed\":false,\"errors\":[]}\n", run->records);
}

static int run_accepted(void) {
        AcceptedRun run = {
                .seqnum_id = SD_ID128_NULL,
        };

        int r = accepted_run_open(&run);
        if (r >= 0) {
                accepted_run_process_dataset(&run);
                r = run.errors == 0 ? 0 : -EINVAL;
        }
        r = accepted_run_close_journal(&run, r);
        accepted_run_print_result(&run, r);
        accepted_run_free(&run);
        return r;
}

typedef struct RejectionRun {
        MMapCache *cache;
        JournalFile *file;
        FILE *input;
        char *line;
        size_t line_alloc;
        size_t records;
        size_t errors;
        uint64_t seqnum;
        sd_id128_t seqnum_id;
} RejectionRun;

static void rejection_run_free(RejectionRun *run) {
        free(run->line);
        if (run->input)
                fclose(run->input);
}

static int rejection_run_close_journal(RejectionRun *run, int status) {
        int close_r = close_journal(run->cache, run->file);
        run->cache = NULL;
        run->file = NULL;
        if (status >= 0 && close_r < 0)
                return close_r;
        return status;
}

static int rejection_run_open(RejectionRun *run) {
        int r = open_journal(arg_output, 1024ULL * 1024ULL, &run->cache, &run->file);
        if (r < 0) {
                fprintf(stderr, "open journal failed: %s\n", strerror(-r));
                return r;
        }

        run->input = fopen(arg_dataset, "re");
        return run->input ? 0 : -errno;
}

static bool repeat_value_too_large(MatrixJsonVariant *value) {
        uint64_t size = 0;

        return matrix_json_variant_is_object(value) &&
               streq_ptr(string_by_key(value, "kind"), "repeat") &&
               unsigned_by_key(value, "size", &size) >= 0 &&
               size > 4ULL * 1024ULL * 1024ULL;
}

static int append_rejection_raw_payload(
                RejectionRun *run,
                const char *raw_payload) {

        const char *eq = strchr(raw_payload, '=');
        if (!eq || eq == raw_payload)
                return -EINVAL;
        return append_raw_payload(run->file, raw_payload, &run->seqnum, &run->seqnum_id);
}

static int append_rejection_field_payload(
                RejectionRun *run,
                const char *field_name,
                MatrixJsonVariant *input_object) {

        MatrixJsonVariant *value = by_key(input_object, "value");

        if (!value || matrix_json_variant_is_null(value))
                return -EINVAL;
        if (repeat_value_too_large(value))
                return -E2BIG;
        return append_field_payload(run->file, field_name, value, &run->seqnum, &run->seqnum_id);
}

static int append_rejection_input(RejectionRun *run, MatrixJsonVariant *input_object) {
        const char *raw_payload = string_by_key(input_object, "raw_payload");
        const char *field_name = string_by_key(input_object, "field_name");

        if (raw_payload)
                return append_rejection_raw_payload(run, raw_payload);
        if (field_name)
                return append_rejection_field_payload(run, field_name, input_object);
        return -EINVAL;
}

static void rejection_run_record_result(
                RejectionRun *run,
                const char *case_id,
                const char *expected,
                int got) {

        if (got >= 0) {
                fprintf(stderr, "%s: unexpectedly accepted\n", case_id);
                run->errors++;
        } else if (streq(classify_error(got), expected))
                run->records++;
        else {
                fprintf(stderr, "%s: got %s, expected %s\n", case_id, classify_error(got), expected);
                run->errors++;
        }
}

static bool rejection_record_payload(
                MatrixJsonVariant *record,
                const char **ret_case_id,
                const char **ret_expected,
                MatrixJsonVariant **ret_input) {

        *ret_case_id = string_by_key(record, "case_id");
        *ret_expected = string_by_key(record, "expected_error");
        *ret_input = by_key(record, "input");
        return *ret_case_id && *ret_expected && *ret_input;
}

static void rejection_run_process_record(RejectionRun *run, MatrixJsonVariant *record) {
        MatrixJsonVariant *input_object = NULL;
        const char *case_id = NULL;
        const char *expected = NULL;

        if (!rejection_record_payload(record, &case_id, &expected, &input_object)) {
                run->errors++;
                return;
        }

        int got = append_rejection_input(run, input_object);
        rejection_run_record_result(run, case_id, expected, got);
}

static void rejection_run_process_line(RejectionRun *run, const char *line) {
        MatrixJsonVariant *record = NULL;
        int r = matrix_json_parse(line, 0, &record, NULL, NULL);

        if (r < 0) {
                run->errors++;
                return;
        }
        if (is_record_type(record, "rejected"))
                rejection_run_process_record(run, record);
        matrix_json_variant_unref(record);
}

static void rejection_run_process_dataset(RejectionRun *run) {
        while (getline(&run->line, &run->line_alloc, run->input) >= 0)
                rejection_run_process_line(run, run->line);
}

static void rejection_run_print_result(const RejectionRun *run, int status) {
        if (status < 0)
                printf("{\"records\":%zu,\"errors\":[\"failed\"]}\n", run->records);
        else
                printf("{\"records\":%zu,\"errors\":[]}\n", run->records);
}

static int run_rejections(void) {
        RejectionRun run = {
                .seqnum_id = SD_ID128_NULL,
        };

        int r = rejection_run_open(&run);
        if (r >= 0) {
                rejection_run_process_dataset(&run);
                r = run.errors == 0 ? 0 : -EINVAL;
        }
        r = rejection_run_close_journal(&run, r);
        rejection_run_print_result(&run, r);
        rejection_run_free(&run);
        return r;
}

int main(int argc, char **argv) {
        int r;

        log_set_max_level(LOG_WARNING);
        r = parse_args(argc, argv);
        if (r < 0)
                return EXIT_FAILURE;

        r = arg_rejection_mode ? run_rejections() : run_accepted();
        return r < 0 ? EXIT_FAILURE : EXIT_SUCCESS;
}
