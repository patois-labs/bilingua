// ICU MessageFormat strings
export default {
  cart: '{n, plural, offset:1 =0 {Your cart is empty} =1 {One item} other {# items and {extra}}}',
  nested: '{a, select, x {{n, plural, one {# thing} other {# things}}} other {Other}}',
  ordinal: 'You are {n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}} in line',
  quoted: "Use '{' and '}' for braces, it''s fine",
  broken: '{n, plural, one {one} other',
  date: 'Delivered on {when, date, short}',
  count: '{n, number} loaves',
}
