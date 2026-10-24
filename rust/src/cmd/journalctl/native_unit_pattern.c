#define _GNU_SOURCE
#include "../../../../go/cmd/journalctl/unit_pattern_compat.h"

void *journalctl_unit_pattern_new(const char *pattern) {
    return journal_unit_pattern_new(pattern);
}
int journalctl_unit_pattern_needed(const char *pattern) {
    return journal_unit_pattern_needed(pattern);
}
int journalctl_unit_pattern_match(void *pattern, const char *value) {
    return journal_unit_pattern_match(pattern, value);
}
void journalctl_unit_pattern_free(void *pattern) {
    journal_unit_pattern_free(pattern);
}
