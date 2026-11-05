// named exports and a default alias
export const en = {
  hello: 'Hello',
  bye: 'Goodbye',
}, de = {
  hello: 'Hallo',
  bye: 'Tsch\u00fcss',
};
const fr = { hello: 'Bonjour' }
export { fr, en as default }
