# bilingua-xliffpkg

Zipped XLIFF packages for Bilingua: WorldServer / Idiom `.xlz` and `.wsxz` (filter `xlz`), plus the memoQ `.mqxlz`
reading of Localization OS through `bilingua_xliffpkg::mqxlz()` (not in `filters()`; the facade keeps
`bilingua-sdl`'s `mqxlz`). The spec and the baseline is the Localization OS filter `app/filters/xliff_package.py`
on its `cat_xliff` engine: `tests/locos_xliffpkg.rs` ports the 7 cases of `tests/verify_xliff_package_filter.py`
(the export case without the database) and `locos_oracle_golden` checks the unit lists of 7 packages, including
Okapi's own `test1.wsxz`, against LocOS.

**Units**: every member whose name ends in `.mqxliff`, `.sdlxliff`, `.sdlxlf`, `.mxliff`, `.xlf` or `.xliff` goes
through `bilingua-xliff` (segments, `<seg-source>` splits, targets, states, locks, inline codes). Segment ids are
`<path>|<segment id>` with `%` and `|` in the path written `%25` and `%7C`, so no two members can give one id;
`Unit::context.location` is the member path.

**Merge**: nothing translated gives the original bytes. Otherwise only the XLIFF members that changed are rewritten
(same name, date, compression method, attributes and member comment); every other member is raw-copied (its
compressed bytes, CRC, sizes, dates, attributes, extra fields and comment; the writer sets its own version fields
and clears the data-descriptor flag, since the local header then carries the sizes). Member order and the archive
comment are kept. With a `target_lang` and something translated, every `<file>` with no (or a blank)
`target-language` gets one. `merge_with`: `stamp_target_always` stamps even when nothing was translated;
`output_encoding: utf-8` writes a rewritten UTF-16 member as UTF-8 with a UTF-8 declaration.

**Safety**: the B6 `Limits` of the call (bombs, forged sizes, encrypted entries, links, path escapes, entry
count); a duplicate member name is refused whether it is listed plainly, with another case or slash direction, or
hidden from the hand scan behind a zip64 end record or bytes before the archive (the zip reader's own record count);
each XLIFF member passes the XML guard (no entity declarations; a plain DOCTYPE is accepted); at most 200 XLIFF
members of at most 64 MB each; XLIFF 2 members and code pages other than UTF-8 / UTF-16 are refused, as LocOS.

## Differences from Localization OS

1. Finished, approved and confirmed segments are extracted with their targets and states (Brent 11-05: bilingual
   files load finished translations); LocOS protects them. Only real locks (`mq:locked`, `iws lock_status`, SDL
   `locked`, `translate="no"`) set `translate=false` and refuse a write. The adapter selects
   `translate && state in {New, Draft}` for the LocOS unit list, and must filter its writes the same way.
2. `<bpt>`/`<ept>` pairs are `g` pairs here and two `x` tokens in LocOS (shared with `bilingua-xliff`).
3. A segment holding only a placeholder is listed by LocOS (its text is one token); `bilingua-xliff` reads no
   segment there.
4. Duplicate names are compared without case and with `\` read as `/`; LocOS refuses exact duplicates only.
5. A member name that climbs out of the archive (`../x.xlf`) is refused; LocOS carries it (it never extracts it).
6. Untouched members are raw-copied; LocOS recompresses every member once anything changed. A rewritten member
   keeps no extra fields in either.
7. `output_encoding: utf-8` turns a rewritten UTF-16 member into UTF-8 (as `bilingua-xliff`); LocOS always keeps a
   member's UTF-16.
8. A plain DOCTYPE without entity declarations is accepted (the shared B6 XML guard, lead decision 11-05); LocOS
   refuses any DOCTYPE in a member.
