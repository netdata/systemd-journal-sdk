package journal

import "fmt"

// FieldNamePolicy selects the validation layer applied to caller-provided
// journal field names.
type FieldNamePolicy int

const (
	// FieldNamePolicyJournald accepts the trusted field names journald itself
	// can write: uppercase ASCII, digits, underscore, up to 64 bytes, not
	// digit-first, with protected _... fields allowed.
	FieldNamePolicyJournald FieldNamePolicy = iota
	// FieldNamePolicyRaw accepts the field names the journal DATA structure can
	// encode. RAW files are SDK journal files, but names outside the journald
	// policy are not guaranteed to be accepted by stock systemd tooling.
	FieldNamePolicyRaw
	// FieldNamePolicyJournalApp emulates untrusted application input accepted by
	// journald: same syntax as JOURNALD, but protected _... fields are dropped.
	FieldNamePolicyJournalApp
)

func (p FieldNamePolicy) String() string {
	switch p {
	case FieldNamePolicyJournald:
		return "journald"
	case FieldNamePolicyRaw:
		return "raw"
	case FieldNamePolicyJournalApp:
		return "journal-app"
	default:
		return fmt.Sprintf("unknown(%d)", p)
	}
}

func validateFieldNamePolicy(policy FieldNamePolicy) error {
	switch policy {
	case FieldNamePolicyJournald, FieldNamePolicyRaw, FieldNamePolicyJournalApp:
		return nil
	default:
		return fmt.Errorf("%w: unsupported field name policy %d", errInvalidJournal, policy)
	}
}

func validateFieldNameForPolicy(name string, policy FieldNamePolicy) error {
	if err := validateFieldNamePolicy(policy); err != nil {
		return err
	}
	switch policy {
	case FieldNamePolicyRaw:
		return validateRawFieldName(name)
	case FieldNamePolicyJournald:
		return validateJournaldFieldName(name, true)
	case FieldNamePolicyJournalApp:
		return validateJournaldFieldName(name, false)
	default:
		panic("unreachable field name policy")
	}
}

func validateRawFieldName(name string) error {
	if name == "" {
		return errFieldName
	}
	for i := 0; i < len(name); i++ {
		if name[i] == '=' {
			return fmt.Errorf("%w: %q", errFieldName, name)
		}
	}
	return nil
}

func validateJournaldFieldName(name string, allowProtected bool) error {
	if name == "" {
		return errFieldName
	}
	if len(name) > 64 {
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	if name[0] == '_' && !allowProtected {
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	if name[0] >= '0' && name[0] <= '9' {
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		return fmt.Errorf("%w: %q", errFieldName, name)
	}
	return nil
}

func prepareFieldsForPolicy(fields []Field, policy FieldNamePolicy) ([]Field, error) {
	if len(fields) == 0 {
		return nil, errEntryEmpty
	}
	if err := validateFieldNamePolicy(policy); err != nil {
		return nil, err
	}
	if policy == FieldNamePolicyJournalApp {
		filtered := make([]Field, 0, len(fields))
		for _, field := range fields {
			if validateFieldNameForPolicy(field.Name, policy) == nil {
				filtered = append(filtered, field)
			}
		}
		if len(filtered) == 0 {
			return nil, errEntryEmpty
		}
		return filtered, nil
	}
	for _, field := range fields {
		if err := validateFieldNameForPolicy(field.Name, policy); err != nil {
			return nil, err
		}
	}
	return fields, nil
}

func logWriterFieldNamePolicy(policy FieldNamePolicy) FieldNamePolicy {
	if policy == FieldNamePolicyRaw {
		return FieldNamePolicyRaw
	}
	return FieldNamePolicyJournald
}
