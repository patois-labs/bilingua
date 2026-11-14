import type { LocaleMessages } from '../types'

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
