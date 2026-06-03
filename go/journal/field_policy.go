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
	return validateFieldNameForPolicyBytes([]byte(name), policy)
}

func validateFieldNameForPolicyBytes(name []byte, policy FieldNamePolicy) error {
	if err := validateFieldNamePolicy(policy); err != nil {
		return err
	}
	switch policy {
	case FieldNamePolicyRaw:
		return validateRawFieldNameBytes(name)
	case FieldNamePolicyJournald:
		return validateJournaldFieldNameBytes(name, true)
	case FieldNamePolicyJournalApp:
		return validateJournaldFieldNameBytes(name, false)
	default:
		panic("unreachable field name policy")
	}
}

func validateRawFieldName(name string) error {
	return validateRawFieldNameBytes([]byte(name))
}

func validateRawFieldNameBytes(name []byte) error {
	if len(name) == 0 {
		return errFieldName
	}
	for i := 0; i < len(name); i++ {
		if name[i] == '=' {
			return fmt.Errorf("%w: %q", errFieldName, string(name))
		}
	}
	return nil
}

func validateJournaldFieldName(name string, allowProtected bool) error {
	return validateJournaldFieldNameBytes([]byte(name), allowProtected)
}

func validateJournaldFieldNameBytes(name []byte, allowProtected bool) error {
	if len(name) == 0 {
		return errFieldName
	}
	if len(name) > 64 {
		return fmt.Errorf("%w: %q", errFieldName, string(name))
	}
	if name[0] == '_' && !allowProtected {
		return fmt.Errorf("%w: %q", errFieldName, string(name))
	}
	if name[0] >= '0' && name[0] <= '9' {
		return fmt.Errorf("%w: %q", errFieldName, string(name))
	}
	for i := 0; i < len(name); i++ {
		if !validJournaldFieldNameByte(name[i]) {
			return fmt.Errorf("%w: %q", errFieldName, string(name))
		}
	}
	return nil
}

func validJournaldFieldNameByte(c byte) bool {
	return c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
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

func prepareRawPayloadsForPolicy(payloads [][]byte, policy FieldNamePolicy) ([][]byte, error) {
	if len(payloads) == 0 {
		return nil, errEntryEmpty
	}
	if err := validateFieldNamePolicy(policy); err != nil {
		return nil, err
	}
	if policy == FieldNamePolicyJournalApp {
		filtered := make([][]byte, 0, len(payloads))
		for _, payload := range payloads {
			name, err := rawPayloadFieldName(payload)
			if err != nil {
				return nil, err
			}
			if validateFieldNameForPolicyBytes(name, policy) == nil {
				filtered = append(filtered, payload)
			}
		}
		if len(filtered) == 0 {
			return nil, errEntryEmpty
		}
		return filtered, nil
	}
	for _, payload := range payloads {
		if err := validateRawPayloadForPolicy(payload, policy); err != nil {
			return nil, err
		}
	}
	return payloads, nil
}

func validateRawPayloadForPolicy(payload []byte, policy FieldNamePolicy) error {
	name, err := rawPayloadFieldName(payload)
	if err != nil {
		return err
	}
	return validateFieldNameForPolicyBytes(name, policy)
}

func rawPayloadFieldName(payload []byte) ([]byte, error) {
	for i, b := range payload {
		if b == '=' {
			if i == 0 {
				return nil, errFieldName
			}
			return payload[:i], nil
		}
	}
	return nil, errFieldName
}

func logWriterFieldNamePolicy(policy FieldNamePolicy) FieldNamePolicy {
	if policy == FieldNamePolicyRaw {
		return FieldNamePolicyRaw
	}
	return FieldNamePolicyJournald
}
