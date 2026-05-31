# SOW-0064 Recent/Current systemd Version Matrix

- Created: `2026-05-31T17:25:33Z`
- Latest stable source: `https://github.com/systemd/systemd/releases/tag/v260.2`
- Sensitive data policy: `reports contain synthetic case IDs, counts, digests, feature flags, statuses, command output hashes, and no raw journal payload values`
- Generated files: `48`
- Passed files: `48`
- Discrepancies: `0`

## Versions

| label | tag | commit | build | journalctl |
| --- | --- | --- | --- | --- |
| recent-production | `v258.8` | `8d9de518e84872e29a6339bbc56a51e0e471d930` | `ok` | `systemd 258 (258.8)` |
| latest-stable | `v260.2` | `f1d0952a125b96b7ab2f1ff29a87448ade8ac29b` | `ok` | `systemd 260 (260.2)` |

## Feature Coverage

| tag | files | compact off/on | keyed off/on | zstd off/on | states | FSS |
| --- | --- | --- | --- | --- | --- | --- |
| `v258.8` | `24 + 2 sealed supplement` | `True` | `True` | `True` | `archived,offline,online` | `covered by tests/systemd_matrix/reports/sealed-fss-smoke-report.md` |
| `v260.2` | `24 + 2 sealed supplement` | `True` | `True` | `True` | `archived,offline,online` | `covered by tests/systemd_matrix/reports/sealed-fss-smoke-report.md` |

## Result Rows

| tag | file_id | status | compact | keyed | compression | state | entries | payloads | digest parity |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `v258.8` | `91c3266703346bf6e481964a` | `ok` | `False` | `False` | `none` | `online` | `8` | `99` | `True` |
| `v258.8` | `3358f1c94a495a842c211094` | `ok` | `False` | `False` | `none` | `offline` | `8` | `99` | `True` |
| `v258.8` | `9541ab35e38beb92d0081da1` | `ok` | `False` | `False` | `none` | `archived` | `8` | `99` | `True` |
| `v258.8` | `bebc314ffcd17de33ce57019` | `ok` | `False` | `False` | `zstd` | `online` | `8` | `99` | `True` |
| `v258.8` | `fa0448ca0579990bd7b41ee4` | `ok` | `False` | `False` | `zstd` | `offline` | `8` | `99` | `True` |
| `v258.8` | `d29aaacd09d6e2d2c04319a3` | `ok` | `False` | `False` | `zstd` | `archived` | `8` | `99` | `True` |
| `v258.8` | `26beddc3da7c9c0ba2939c39` | `ok` | `False` | `True` | `none` | `online` | `8` | `99` | `True` |
| `v258.8` | `35be209edf072fc24b80c5f3` | `ok` | `False` | `True` | `none` | `offline` | `8` | `99` | `True` |
| `v258.8` | `c8eb345cd17c70c198df8384` | `ok` | `False` | `True` | `none` | `archived` | `8` | `99` | `True` |
| `v258.8` | `13c860f7ca81a484197c33dc` | `ok` | `False` | `True` | `zstd` | `online` | `8` | `99` | `True` |
| `v258.8` | `96276173a59394a77d9c5fe9` | `ok` | `False` | `True` | `zstd` | `offline` | `8` | `99` | `True` |
| `v258.8` | `4e091b41d191694a1a53ea45` | `ok` | `False` | `True` | `zstd` | `archived` | `8` | `99` | `True` |
| `v258.8` | `38963e14197307c6b17ba869` | `ok` | `True` | `False` | `none` | `online` | `8` | `99` | `True` |
| `v258.8` | `5e233b20dcf6f60ba84fc82b` | `ok` | `True` | `False` | `none` | `offline` | `8` | `99` | `True` |
| `v258.8` | `e4fbeb08a16c6e87481c2557` | `ok` | `True` | `False` | `none` | `archived` | `8` | `99` | `True` |
| `v258.8` | `a75a8b734e228b549a0bc624` | `ok` | `True` | `False` | `zstd` | `online` | `8` | `99` | `True` |
| `v258.8` | `57d66fc99986f1bf113476cb` | `ok` | `True` | `False` | `zstd` | `offline` | `8` | `99` | `True` |
| `v258.8` | `01ab14fd3aa5130902fb3bed` | `ok` | `True` | `False` | `zstd` | `archived` | `8` | `99` | `True` |
| `v258.8` | `21268463f16cb7814f6c67db` | `ok` | `True` | `True` | `none` | `online` | `8` | `99` | `True` |
| `v258.8` | `147c2f82358319d8b3d43000` | `ok` | `True` | `True` | `none` | `offline` | `8` | `99` | `True` |
| `v258.8` | `e071c3c9e5355faa4bdb1245` | `ok` | `True` | `True` | `none` | `archived` | `8` | `99` | `True` |
| `v258.8` | `06511abefa7bfb94b72f469d` | `ok` | `True` | `True` | `zstd` | `online` | `8` | `99` | `True` |
| `v258.8` | `e3957d42d3b1ee111a21fbf9` | `ok` | `True` | `True` | `zstd` | `offline` | `8` | `99` | `True` |
| `v258.8` | `1da01cb7844d6a35e98e2e4b` | `ok` | `True` | `True` | `zstd` | `archived` | `8` | `99` | `True` |
| `v260.2` | `a418dc1baf2243b039ec69ce` | `ok` | `False` | `False` | `none` | `online` | `8` | `99` | `True` |
| `v260.2` | `1015c3b4446bd2681aae4d28` | `ok` | `False` | `False` | `none` | `offline` | `8` | `99` | `True` |
| `v260.2` | `0784c068bf7389aa17b77bb8` | `ok` | `False` | `False` | `none` | `archived` | `8` | `99` | `True` |
| `v260.2` | `c8fd1282bcd1b60f08222446` | `ok` | `False` | `False` | `zstd` | `online` | `8` | `99` | `True` |
| `v260.2` | `37409d612b824889e5b50c11` | `ok` | `False` | `False` | `zstd` | `offline` | `8` | `99` | `True` |
| `v260.2` | `3be79b901f41ce53b93fc19d` | `ok` | `False` | `False` | `zstd` | `archived` | `8` | `99` | `True` |
| `v260.2` | `2afa4bcd9c342d3d6256b8b7` | `ok` | `False` | `True` | `none` | `online` | `8` | `99` | `True` |
| `v260.2` | `336c2f33e86f9ebf807ffae1` | `ok` | `False` | `True` | `none` | `offline` | `8` | `99` | `True` |
| `v260.2` | `324fa56913877e00b080d17c` | `ok` | `False` | `True` | `none` | `archived` | `8` | `99` | `True` |
| `v260.2` | `e90ccedf39dc29c681b17228` | `ok` | `False` | `True` | `zstd` | `online` | `8` | `99` | `True` |
| `v260.2` | `71a1fbab044c8acf0434b361` | `ok` | `False` | `True` | `zstd` | `offline` | `8` | `99` | `True` |
| `v260.2` | `ba724f78f3adf561bcbd0228` | `ok` | `False` | `True` | `zstd` | `archived` | `8` | `99` | `True` |
| `v260.2` | `9be0607aaaee4dfd3b849ec0` | `ok` | `True` | `False` | `none` | `online` | `8` | `99` | `True` |
| `v260.2` | `e969710e158a54aa14ecb930` | `ok` | `True` | `False` | `none` | `offline` | `8` | `99` | `True` |
| `v260.2` | `601bca92e01bd976c2e21dd9` | `ok` | `True` | `False` | `none` | `archived` | `8` | `99` | `True` |
| `v260.2` | `723a952fe1cee88f86fddb2e` | `ok` | `True` | `False` | `zstd` | `online` | `8` | `99` | `True` |
| `v260.2` | `ab743b928ad6934db42fa197` | `ok` | `True` | `False` | `zstd` | `offline` | `8` | `99` | `True` |
| `v260.2` | `358937cc54176423bc384164` | `ok` | `True` | `False` | `zstd` | `archived` | `8` | `99` | `True` |
| `v260.2` | `1d52a1e1e86f4f11918714bb` | `ok` | `True` | `True` | `none` | `online` | `8` | `99` | `True` |
| `v260.2` | `0a1ecdfbd28ec887d3887191` | `ok` | `True` | `True` | `none` | `offline` | `8` | `99` | `True` |
| `v260.2` | `9a576735778bc5ae5d605628` | `ok` | `True` | `True` | `none` | `archived` | `8` | `99` | `True` |
| `v260.2` | `64ba1ef65c68607e2e5a9968` | `ok` | `True` | `True` | `zstd` | `online` | `8` | `99` | `True` |
| `v260.2` | `606993cd5b5b9112d90706e2` | `ok` | `True` | `True` | `zstd` | `offline` | `8` | `99` | `True` |
| `v260.2` | `209204e21872a3e34ed999b4` | `ok` | `True` | `True` | `zstd` | `archived` | `8` | `99` | `True` |

## Blockers And Limits

- `fss_not_generated`: resolved by
  `tests/systemd_matrix/reports/sealed-fss-smoke-report.md`. The runner now
  patches only `.local` systemd source copies so sealed/FSS cases use
  `SYSTEMD_JOURNAL_FSS_ROOT` instead of host journal state.
- `helper_patched_into_local_sources`: the committed matrix framework exists,
  but the systemd internal generator helper is copied only into `.local`
  systemd source checkouts; upstream systemd source trees are not modified.
