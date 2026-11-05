# Wiring for the facade (the lead replaces the wire commit at merge)

| Item | Value |
|---|---|
| Crate / type | `bilingua-arb`, `bilingua_arb::Arb` (unit struct, no options) |
| Format id | `arb` |
| Extensions | `arb` |
| Detection | `sniff`: a JSON object with a string `@@locale`. Put `Box::new(bilingua_arb::Arb)` **before** `bilingua_json::JsonFilter` in `filters()` |
| Options | none; target language comes from `Document::target_lang` |
| Contract test | `bilingua/tests/contract.rs` `crate_of`: `"arb" => "bilingua-arb"` |
| Portability script | add `arb` to `CRATES` in `tools/check-portable.sh` |
| Workspace | `members` and `[workspace.dependencies]` entries; the facade crate depends on `bilingua-arb` |
| Corpus | `oracle/corpus/arb/` (four files; the facade contract test walks every corpus folder) |

Notes for the lead:

- A `.json` file in ARB shape (string `@@locale`) is not routed to this crate by extension. LocOS does that sniff in
  its JSON filter. If wanted, the facade can call `Arb.sniff` for `.json` files before `JsonFilter` (decision for Brent).
- `bl-dump` and the oracle corpus test merge every corpus file for `es-ES`. For ARB that reshapes plurals to
  Spanish categories on purpose (as LocOS does), so the corpus plurals are written with `one`, `many` and `other`
  (and ordinals with `other` only) so the unit count survives. Do not add corpus plurals with other categories.
- The scoreboard row for `{value}` (a message that is only a placeholder) shows one LocOS unit "missing in
  Bilingua": the oracle drops Bilingua units with no letters; the unit exists.
