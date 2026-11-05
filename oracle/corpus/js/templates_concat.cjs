/**
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
