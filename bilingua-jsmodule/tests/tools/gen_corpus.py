"""Writes the made-up corpus oracle/corpus/js (stdlib only; every text is invented for Bilingua, Apache-2.0).

    python gen_corpus.py ..\\..\\..\\oracle\\corpus\\js
"""

import os
import sys

out = sys.argv[1]
os.makedirs(out, exist_ok=True)


def put(name, text, enc="utf-8", bom=b"", newline=None):
    if newline:
        text = text.replace("\n", newline)
    with open(os.path.join(out, name), "wb") as fh:
        fh.write(bom + text.encode(enc))


VUE = r"""// Lumen Bakery storefront messages (vue-i18n).
export default {
  nav: {
    home: 'Home',
    menu: "Our menu",
    about: 'About us',
  },
  greeting: 'Good morning, {name}!',
  loaves: 'no loaves | one loaf | {count} loaves',
  link: 'Back to @:nav.home',
  mail: "Write to us {'@'} lumen.example",
  hours: ['Mon to Fri', 'Saturday', 'Closed on Sunday'],
  loyalty: { points: 3, active: true, tier: null, note: '' },
  'checkout-title': 'Checkout',
  price: 'Total: %s (incl. %1$d items)',
  sale: '50% off today',
  dash: '---',
}
"""
put("vue_messages.js", VUE)

I18N = r"""'use strict';
/* i18next resources for the order page */
module.exports = {
  translation: {
    welcome: "Welcome, {{name}}!",
    unread: 'You have {{count}} unread messages',
    nested: '$t(translation.welcome) Glad you are here.',
    trans: 'Click <1>here</1> to continue',
    format: 'Ready in {{minutes, number}} minutes',
    raw: "Escaped value: {{- html}}",
  },
};
"""
put("i18next_resources.cjs", I18N)

TS = r"""import type { LocaleMessages } from '../types'

export type Messages = typeof en

interface Extra {
  note?: string
}

const shared = {
  ok: 'OK',
  cancel: `Cancel`,
} as const

export const en: LocaleMessages = {
  shared,
  title: `Multi-line
template title`,
  items: '{count, plural, =0 {No items} one {# item} other {# items}}',
  gender: '{gender, select, female {She replied} male {He replied} other {They replied}}',
  concat: 'Part one, ' + "part two",
} satisfies LocaleMessages

export default en
"""
put("typed_messages.ts", TS)

ICU = r"""// ICU MessageFormat strings
export default {
  cart: '{n, plural, offset:1 =0 {Your cart is empty} =1 {One item} other {# items and {extra}}}',
  nested: '{a, select, x {{n, plural, one {# thing} other {# things}}} other {Other}}',
  ordinal: 'You are {n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}} in line',
  quoted: "Use '{' and '}' for braces, it''s fine",
  broken: '{n, plural, one {one} other',
  date: 'Delivered on {when, date, short}',
  count: '{n, number} loaves',
}
"""
put("icu_plurals.mjs", ICU)

NAMED = r"""// named exports and a default alias
export const en = {
  hello: 'Hello',
  bye: 'Goodbye',
}, de = {
  hello: 'Hallo',
  bye: 'Tsch\u00fcss',
};
const fr = { hello: 'Bonjour' }
export { fr, en as default }
"""
put("named_exports.js", NAMED)

TEMPL = r"""/**
 * Strings in every quote style.
 */
const base = {
  plain: 'single',
  dq: "double",
  tpl: `template`,
  cat: 'one ' + "two " + `three`,
  multi: `first line
second line
  third line`,
  esc: `price \${5} and \`tick\``,
}; // trailing comment
exports.messages = base
module.exports.extra = { late: 'Late addition' }
"""
put("templates_concat.cjs", TEMPL)

ESC = r"""export default {
  quote: 'It\'s "fine"',
  back: "C:\\temp\\file",
  uni: 'caf\u00e9 \u{1F600} \uD83D\uDE00',
  hex: '\x41\x42 \0 end',
  cont: 'long \
line',
  tabs: 'a\tb\nc',
  raw_unicode: 'caf\u00e9 \u65e5\u672c',
}
"""
put("escapes.js", ESC)

put("crlf_bom.js", VUE, newline="\r\n", bom=b"\xef\xbb\xbf")
put("crlf.ts", TS, newline="\r\n")
put("cp1252.js", "// Legacy export\nexport default {\n  title: 'Caf\u00e9 cr\u00e8me \u20ac3',\n  quote: '\u201cSmart\u201d quotes',\n}\n", enc="cp1252")
put("utf16le.js", "export default {\n  title: 'Caf\u00e9',\n  jp: '\u65e5\u672c\u8a9e',\n}\n", enc="utf-16-le", bom=b"\xff\xfe")
put("utf16be.mjs", "export default {\n  title: 'Caf\u00e9',\n}\n", enc="utf-16-be", bom=b"\xfe\xff")

LANGS = r"""// The eleven test languages.
export default {
  en: 'Welcome to Larchmoor',
  es: 'Bienvenido a Larchmoor',
  fr: 'Bienvenue \u00e0 Larchmoor, {name}',
  it: "Benvenuti a Larchmoor",
  de: `Willkommen in Larchmoor`,
  nl: 'Welkom in Larchmoor',
  pt: 'Bem-vindo a Larchmoor',
  ja: 'ラーチムアへようこそ、{name}さん',
  zh: '欢迎来到拉奇莫尔',
  ko: '라치모어에 오신 것을 환영합니다',
  ar: 'مرحبا بكم في لارتشمور، {name}',
}
"""
put("languages.js", LANGS)
put("arabic_rtl.mts", "export default {\n  title: 'مرحبا',\n  items: ['الخبز', 'الحليب'],\n  count: '{n} عناصر',\n}\n")
put("japanese.cts", "module.exports = {\n  title: 'こんにちは',\n  list: ['パン', 'ミルク'],\n}\n")

SHEBANG = "#!/usr/bin/env node\n'use strict'\nconst m = { a: 'Shebang file' }\nmodule.exports = m\n"
put("shebang_asi.js", SHEBANG)
print("corpus written to", out)


# Files the filter must refuse live in tests/data, not in the corpus (the corpus holds only files that open).
data = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")
os.makedirs(data, exist_ok=True)
with open(os.path.join(data, "not_a_module.js"), "wb") as fh:
    fh.write(b"import x from './x'\nexport default { a: x }\n")
with open(os.path.join(data, "qt_linguist.ts"), "wb") as fh:
    fh.write(b'<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE TS>\n<TS version="2.1" language="de_DE">\n<context>\n  <name>Main</name>\n  <message><source>Open</source><translation>Offnen</translation></message>\n</context>\n</TS>\n')
