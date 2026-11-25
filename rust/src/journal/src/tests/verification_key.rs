use crate::parse_verification_key;

const SEED: &str = "0123456789abcdef01234567";

// Accepted and rejected shapes were checked against the systemd v262 CLI.
#[test]
fn parse_verification_key_follows_systemd_grammar() {
    let accepted = [
        (format!("{SEED}/1-f4240"), 1, 0xf4240),
        ("-012345-6789ab--cdef01234567/1-2".to_string(), 1, 2),
        (format!("{SEED}/1-2x"), 1, 2),
        (format!("{SEED}/ 1- 2"), 1, 2),
        (format!("{SEED}/+1-+2"), 1, 2),
        (format!("{SEED}/0x1-0X2"), 1, 2),
        (format!("{SEED}/1--1"), 1, u64::MAX),
        (format!("{SEED}/1-1ffffffffffffffff"), 1, u64::MAX),
    ];
    for (key, start, interval) in accepted {
        let parsed = parse_verification_key(&key).unwrap_or_else(|err| panic!("{key}: {err}"));
        assert_eq!(
            parsed.seed,
            [
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67
            ],
            "{key}"
        );
        assert_eq!(parsed.start_epoch, start, "{key}");
        assert_eq!(parsed.interval_usec, interval, "{key}");
    }

    let rejected = [
        String::new(),
        format!("+a{}/1-2", &SEED[2..]),
        format!("{SEED}ab/1-2"),
        SEED.to_string(),
        format!("{SEED}/1"),
        format!("{SEED}/1 -2"),
        format!("{SEED}/0-2"),
        format!("{SEED}/1-0"),
        format!("{SEED}/0x-2"),
        format!("{SEED}/ffffffffffffffff-2"),
    ];
    for key in rejected {
        assert!(parse_verification_key(&key).is_err(), "{key}");
    }
}
