#ifndef JOURNALCTL_UNIT_PATTERN_COMPAT_H
#define JOURNALCTL_UNIT_PATTERN_COMPAT_H

#include <stdint.h>
#include <fnmatch.h>
#include <locale.h>
#include <limits.h>
#ifdef __APPLE__
/* Darwin needs xlocale for the thread-local MB_CUR_MAX definition. */
#include <xlocale.h>
#endif
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <wctype.h>

/* CLI-only FreeBSD fallback for brackets missing from its native fnmatch. */
struct journal_unit_pattern {
    wchar_t *bytes, *wide, *value;
    size_t capacity;
    int posix;
};

static int journal_unit_pattern_needed(const char *p) {
    return strstr(p, "[:") || strstr(p, "[.") || strstr(p, "[=");
}

/* A successful member still needs a closing bracket, but not valid later members. */
static const wchar_t *journal_unit_bracket_end(const wchar_t *p, int *invalid) {
    while (*p && *p != ']') {
        if (p[0] == '[' && (p[1] == ':' || p[1] == '.' || p[1] == '=')) {
            wchar_t marker = p[1];
            const wchar_t *end = p + 2;
            while (*end && !(end[0] == marker && end[1] == ']')) end++;
            if (!*end && marker != ':') { *invalid = 1; return NULL; }
            if (*end) {
                if (marker == '=' && end != p + 3) { *invalid = 1; return NULL; }
                if (marker == ':') {
                    const wchar_t *name = p + 2;
                    while (name < end && *name >= 'a' && *name < 'z') name++;
                    if (name != end) { p++; continue; }
                    if (end - p >= 257) { *invalid = 1; return NULL; }
                }
                p = end + 2; continue;
            }
        }
        p++;
    }
    return *p ? p + 1 : NULL;
}

/* FreeBSD and glibc wide collation keys separate weight levels with 1. */
static int journal_unit_equivalent(wchar_t left, wchar_t right, int bytes) {
    if (left == right) return 1;
    if (bytes) {
        wint_t a = btowc((unsigned char)left), b = btowc((unsigned char)right);
        if (a == WEOF || b == WEOF) return 0;
        left = (wchar_t)a; right = (wchar_t)b;
    }
    wchar_t a[] = {left, 0}, b[] = {right, 0};
    wchar_t local_a[32], local_b[32], *key_a = local_a, *key_b = local_b;
    size_t na = wcsxfrm(local_a, a, 32), nb = wcsxfrm(local_b, b, 32);
    if (na == SIZE_MAX || nb == SIZE_MAX || na >= SIZE_MAX / sizeof(wchar_t) || nb >= SIZE_MAX / sizeof(wchar_t)) return 0;
    if (na >= 32) {
        key_a = malloc((na + 1) * sizeof(wchar_t));
        if (!key_a) return 0;
        if (wcsxfrm(key_a, a, na + 1) > na) { free(key_a); return 0; }
    }
    if (nb >= 32) {
        key_b = malloc((nb + 1) * sizeof(wchar_t));
        if (!key_b) { if (key_a != local_a) free(key_a); return 0; }
        if (wcsxfrm(key_b, b, nb + 1) > nb) {
            if (key_a != local_a) free(key_a);
            free(key_b); return 0;
        }
    }
    size_t i = 0;
    while (key_a[i] > 1 && key_b[i] > 1 && key_a[i] == key_b[i]) i++;
    int equal = key_a[i] <= 1 && key_b[i] <= 1;
    if (key_a != local_a) free(key_a);
    if (key_b != local_b) free(key_b);
    return equal;
}

/* Keep range ordering consistent with the platform's ordinary matcher. */
static int journal_unit_range(wchar_t low, wchar_t high, wchar_t value, int bytes) {
    wchar_t pattern[] = {'[', '\\', low, '-', '\\', high, ']', 0};
    wchar_t candidate[] = {value, 0};
    char encoded[7 * MB_LEN_MAX + 1], text[MB_LEN_MAX + 1];
    if (bytes) {
        for (size_t i = 0; i < sizeof(pattern) / sizeof(*pattern); i++) encoded[i] = (char)pattern[i];
        text[0] = (char)value; text[1] = 0;
    } else {
        mbstate_t state = {0}; const wchar_t *source = pattern;
        if (wcsrtombs(encoded, &source, sizeof(encoded), &state) == (size_t)-1 || source) return 0;
        memset(&state, 0, sizeof(state)); source = candidate;
        if (wcsrtombs(text, &source, sizeof(text), &state) == (size_t)-1 || source) return 0;
    }
    return fnmatch(encoded, text, 0) == 0;
}

static int journal_unit_bracket(const wchar_t **cursor, const wchar_t *candidate, int bytes, int posix) {
    wchar_t value = *candidate;
    const wchar_t *start = *cursor, *p = start + 1;
    int negate = *p == '!' || (!posix && *p == '^');
    if (negate) p++;
    int first = 1;
    while (*p && (first || *p != ']')) {
        first = 0;
        wchar_t low = *p++;
        int member = 0, character = 1;
        if (low == '[' && *p == ':') {
            const wchar_t *end = p + 1;
            char name[256]; size_t n = 0;
            while (*end >= 'a' && *end < 'z' && n < sizeof(name) - 1) name[n++] = (char)*end++;
            if (n == sizeof(name) - 1) return 0;
            if (*end == ':' && end[1] == ']') {
                name[n] = 0;
                wctype_t type = wctype(name);
                if (!type) return 0;
                member = iswctype(bytes ? btowc((unsigned char)value) : (wint_t)value, type) != 0;
                p = end + 2;
                character = 0;
            }
        } else if (low == '[' && (*p == '.' || *p == '=')) {
            wchar_t marker = *p;
            const wchar_t *end = p + 1;
            while (*end && !(end[0] == marker && end[1] == ']')) end++;
            if (*end && end == p + 2) {
                low = p[1]; p = end + 2;
                if (marker == '=') {
                    if (bytes && MB_CUR_MAX > 1) {
                        char raw[MB_LEN_MAX]; size_t n = 0;
                        while (n < sizeof(raw) && candidate[n]) { raw[n] = (char)candidate[n]; n++; }
                        mbstate_t state = {0}; wchar_t decoded;
                        size_t used = mbrtowc(&decoded, raw, n, &state);
                        wint_t first = btowc((unsigned char)low);
                        member = first != WEOF && used != (size_t)-1 && used != (size_t)-2 &&
                            journal_unit_equivalent((wchar_t)first, decoded, 0);
                    } else member = journal_unit_equivalent(low, value, bytes);
                    character = 0;
                }
            } else if (marker == '.') {
                return 0;
            }
        }
        if (character) {
            if (*p == '-' && !p[1] && low != value) return 0;
            if (*p == '-' && p[1] && p[1] != ']') {
                p++;
                wchar_t high = *p++;
                if (high == '[' && *p == '.') {
                    if (p[1] && p[2] == '.' && p[3] == ']') { high = p[1]; p += 4; }
                    else return 0;
                }
                member = journal_unit_range(low, high, value, bytes);
            } else member = low == value;
        }
        if (member) {
            int invalid = 0;
            const wchar_t *end = journal_unit_bracket_end(p, &invalid);
            if (invalid) return 0;
            if (!end) { *cursor = start + 1; return value == '['; }
            *cursor = end;
            return !negate;
        }
    }
    if (!*p) { *cursor = start + 1; return value == '['; }
    *cursor = p + 1;
    return negate;
}

static int journal_unit_wildcard(const wchar_t *pattern, const wchar_t *value, int bytes, int posix) {
    const wchar_t *p = pattern, *v = value, *star = NULL, *retry = NULL;
    for (;;) {
        if (*p == '*') {
            while (*p == '*') p++;
            if (!*p) return 1;
            star = p; retry = v;
            continue;
        }
        if (!*p && !*v) return 1;
        const wchar_t *next = p + (*p != 0);
        int matches = *v && (*p == '?' || *p == *v);
        if (*p == '[' && *v) {
            next = p;
            matches = journal_unit_bracket(&next, v, bytes, posix);
        }
        if (matches) { p = next; v++; continue; }
        if (!star || !*retry) return 0;
        v = ++retry; p = star;
    }
}

static void journal_unit_pattern_free(struct journal_unit_pattern *p) {
    if (!p) return;
    free(p->bytes); free(p->wide); free(p->value); free(p);
}

static struct journal_unit_pattern *journal_unit_pattern_new(const char *pattern) {
    size_t n = strlen(pattern) + 1; /* Flawfinder: ignore -- Both CLI callers use terminated CString storage. */
    if (n > SIZE_MAX / sizeof(wchar_t)) return NULL;
    struct journal_unit_pattern *p = calloc(1, sizeof(*p));
    if (!p) return NULL;
    p->posix = getenv("POSIXLY_CORRECT") != NULL;
    p->bytes = malloc(n * sizeof(wchar_t));
    if (!p->bytes) { journal_unit_pattern_free(p); return NULL; }
    for (size_t i = 0; i < n; i++) p->bytes[i] = (unsigned char)pattern[i];
    if (MB_CUR_MAX > 1) {
        p->wide = malloc(n * sizeof(wchar_t));
        if (!p->wide) { journal_unit_pattern_free(p); return NULL; }
        mbstate_t state = {0}; const char *source = pattern;
        if (mbsrtowcs(p->wide, &source, n, &state) == (size_t)-1) { free(p->wide); p->wide = NULL; }
    }
    return p;
}

static int journal_unit_pattern_match(struct journal_unit_pattern *p, const char *value) {
    size_t n = strlen(value) + 1; /* Flawfinder: ignore -- Both CLI callers append a NUL to the reused buffer. */
    if (n > p->capacity) {
        if (n > SIZE_MAX / sizeof(wchar_t)) return 0;
        wchar_t *buffer = realloc(p->value, n * sizeof(wchar_t));
        if (!buffer) return 0;
        p->value = buffer; p->capacity = n;
    }
    if (p->wide) {
        mbstate_t state = {0}; const char *source = value;
        if (mbsrtowcs(p->value, &source, n, &state) != (size_t)-1 &&
            journal_unit_wildcard(p->wide, p->value, 0, p->posix)) return 1;
    }
    /* glibc retries byte matching after an unsuccessful multibyte match. */
    for (size_t i = 0; i < n; i++) p->value[i] = (unsigned char)value[i];
    return journal_unit_wildcard(p->bytes, p->value, 1, p->posix);
}
#endif
