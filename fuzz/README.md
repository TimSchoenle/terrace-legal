# Fuzzing

Four targets, each a shim over an oracle in `src/oracle/`:

| Target | Input | Checks |
|---|---|---|
| `accept_language` | header bytes | Parsing never panics, stays inside its bounds and repeats nothing, and negotiation picks the client's first choice |
| `locale_tag` | text | An accepted tag is canonical and survives its own text, `_` and serde, and truncates to its language |
| `config_load` | JSON `LegalConfig` | Validation never panics, a refusal names something, and everything a valid catalog lists can be served |
| `markdown` | text | Nothing in the rendered output can execute |

## Replaying

`cargo test` in this directory replays every file under `seeds/` and `corpus/` through its
oracle, then runs a deterministic sweep of generated inputs. It needs no sanitizer. A failure
names the test, so a reproducer is promoted by copying the input into `corpus/<target>/`.

`LEGAL_FUZZ_ITERATIONS` sets the sweep length. CI raises it from the default of 500.

## Running a campaign

From the repository root, on nightly:

```sh
cargo +nightly fuzz run accept_language fuzz/corpus/accept_language fuzz/seeds/accept_language \
  -- -dict=fuzz/dictionaries/legal.dict -max_total_time=60
```

`cargo fuzz` writes new inputs into the first corpus directory it is given. A finding goes into
`corpus/<target>/` with the fix that made it pass.
