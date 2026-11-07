"""Type stubs for the Bilingua Python module (the compiled extension `bilingua`).

A Document is plain dicts and lists, the same JSON shape as the Rust `bilingua_model::Document`. The TypedDicts
below name the keys a caller reads and writes; any key may be missing in an older document, and new keys may be
added in a 1.x release (`total=False`).
"""

from typing import Any, Literal, Mapping, TypedDict

__version__: str

Granularity = Literal["sentence", "paragraph", "document"]
OutputEncoding = Literal["keep", "utf-8", "utf8"]
SegState = Literal["new", "draft", "translated", "confirmed", "reviewed", "signed-off", "rejected", "locked"]
FormatOptionValue = str | bool | int

class TextInline(TypedDict):
    text: str

class TagBody(TypedDict, total=False):
    id: str
    kind: Literal["open", "close", "standalone"]
    display: str
    free: bool

class TagInline(TypedDict):
    tag: TagBody

Inline = TextInline | TagInline

class Origin(TypedDict, total=False):
    kind: Literal["tm", "mt", "ai", "human", "other"]
    name: str
    match_pct: int

class Segment(TypedDict, total=False):
    id: str
    source: list[Inline]
    target: list[Inline]
    state: SegState
    origin: Origin
    empty_target: bool

class UnitContext(TypedDict, total=False):
    kind: str | None
    style: str | None
    key: str | None
    heading: str | None
    location: str | None
    max_len: int | None
    notes: list[str]
    extra: dict[str, str]

class Note(TypedDict, total=False):
    author: str
    text: str
    kind: str

class Unit(TypedDict, total=False):
    id: str
    segments: list[Segment]
    separators: list[str]
    codes: dict[str, dict[str, Any]]
    context: UnitContext
    translate: bool
    notes: list[Note]
    lock_reason: str
    hidden: dict[str, str]

class Warning(TypedDict, total=False):
    kind: str
    part: str
    message: str

class ParserRecord(TypedDict, total=False):
    """What made a Document (`extract` adds it as `parser`; `merge` ignores it)."""
    bilingua: str
    format: str
    crate: str | None
    crate_version: str | None
    schema: int | None
    profile: str | None
    profile_version: int | None

class Document(TypedDict, total=False):
    parser: ParserRecord
    format: str
    name: str | None
    source_lang: str | None
    target_lang: str | None
    granularity: Granularity
    units: list[Unit]
    skeleton: dict[str, Any]
    warnings: list[Warning]
    source_encoding: str

class FormatInfo(TypedDict):
    id: str
    extensions: list[str]
    bilingual: bool

class BilinguaError(Exception):
    """Every Bilingua failure. `code` is stable: "parse", "unsupported", "unknown_segment", "locked",
    "invented_tag", "duplicated_tag", "parser_version", "write" (a broken or foreign skeleton), "unwritable" (the
    translation cannot be written in this format: the translator has to change it), "limit", "encoding", "io",
    "invalid_argument" (also an unknown format option or profile), "invalid_document", "internal"; "error" for a
    failure a later release gives its own code. `detail` is the parser's own sentence: the message without the
    `not a <format> file:` prefix of a "parse" error, the message itself for every other code. Size and budget
    refusals are "limit", a file whose bytes cannot be read as text "encoding"."""

    code: str
    detail: str

def formats() -> list[FormatInfo]:
    """All formats, most specific first."""

def detect(name: str, data: bytes) -> str | None:
    """The format id for a file name and its bytes, or None when no parser accepts it."""

def extract(
    name: str,
    data: bytes,
    *,
    source_lang: str | None = None,
    target_lang: str | None = None,
    granularity: Granularity = "paragraph",
    limits: Mapping[str, int] | None = None,
    format_options: Mapping[str, FormatOptionValue] | None = None,
    profile: str | None = None,
    profile_version: int | None = None,
) -> Document:
    """Opens a file. `limits` changes any of the safety limits by name (`max_units`, ...). `format_options` are
    `"<format>.<name>"` keys (`"csv.columns"`, `"odf.translate_comments"`) to str, bool or int; a key no format
    declares, a value of the wrong kind, or an unknown `profile` raises BilinguaError with code
    "invalid_argument". `profile="locos"` gives Localization OS's behaviour; `profile="locos@1"` or
    `profile_version=1` asks for that version of the profile (a version this release cannot produce raises code
    "parser_version"). The Document carries `parser`: the Bilingua release, the format crate, its version and
    skeleton schema, and the profile and its version, for the caller to stamp."""

def merge(
    document: Mapping[str, Any],
    *,
    output_encoding: OutputEncoding | None = None,
    stamp_target_always: bool | None = None,
    profile: str | None = None,
    format_options: Mapping[str, FormatOptionValue] | None = None,
    profile_version: int | None = None,
) -> bytes:
    """Writes a Document back to file bytes. Every option is keyword-only. `output_encoding="utf-8"` writes UTF-8
    whatever the file's own encoding ("keep" is the default); `stamp_target_always` stamps the target language and
    fills plurals even when nothing is translated (default False). `profile="locos"` starts from Localization OS's
    options (UTF-8 out except ini, strings, dxf, txt and properties, stamp always); `output_encoding` and
    `stamp_target_always` given here override the profile's. An unknown profile or format option raises BilinguaError with code
    "invalid_argument". The profile version is `profile_version`, else the one in `profile` (`"locos@1"`), else the
    one in the Document's `parser` record, else the current one; one this release cannot produce raises
    "parser_version"."""

def profiles() -> dict[str, int]:
    """The profiles Bilingua knows, name to version (`{"locos": 1}`)."""

def version() -> str:
    """The Bilingua version."""

class LocosCode(TypedDict, total=False):
    id: str
    letter: str
    open: str | None
    close: str | None
    free: bool
    unpaired: Literal["open", "close"]

LocosMap = dict[str, LocosCode]

class TokenProblem(TypedDict, total=False):
    kind: Literal["stray", "unknown", "kind_mismatch", "repeated", "stray_close", "unclosed", "pair_as_point"]
    token: str
    at: int

class LocosUnit(TypedDict):
    p_index: int
    segment_id: str
    text: str
    map: LocosMap
    style: str

def to_locos_text(format: str, inlines: list[Inline]) -> tuple[str, LocosMap]:
    """A segment's inline list as Localization OS text (tokens such as `⟦g1⟧`) and its token map,
    numbered by the LocOS rule of `format`: pairs by id for html, the tag ids as numbers for docx, pptx and xlsx,
    in reading order otherwise; a number that look-alike text in the source uses is skipped."""

def from_locos_text(text: str, map: LocosMap) -> tuple[list[Inline], list[TokenProblem]]:
    """Localization OS text back to inlines with the map `to_locos_text` gave. A malformed token, one naming no
    code in the map, or one of the wrong kind stays in the text as it is and is listed."""

def locos_units(document: Mapping[str, Any]) -> list[LocosUnit]:
    """The translatable segments of a Document extracted with `profile="locos"`, numbered as Localization OS
    numbers its units (`locos.p_index` when every unit has a distinct one, else the position), with their LocOS
    text, token map and style."""

def locos_set_targets(document: Mapping[str, Any], targets: Mapping[str, str]) -> tuple[Document, dict[str, list[TokenProblem]]]:
    """Sets LocOS-text translations by segment id the way Localization OS means them (empty = translated to
    nothing, white space kept, standalone placeholders may repeat). Returns the new Document and, per segment, the
    tokens kept as text and, for the XLIFF family, TXML, TTX and the packages, each tag repair LocOS's writers make
    (`repeated`, `stray_close`: the tag dropped, its text kept; `unclosed`: closed at the end; `pair_as_point`:
    written empty). A target the document cannot take raises BilinguaError naming the segment."""
