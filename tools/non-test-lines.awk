# Prints every non-test line of a Rust file as FILE:LINE: text.
# A column-0 #[cfg(test)] followed by an inline `mod name {` skips that module up to its column-0 `}`;
# any other #[cfg(test)] skips only the next line (a `mod tests;` or a test-only `use`).
skip == 1 { if ($0 ~ /^}/) skip = 0; next }
pend > 0 {
  if (pend == 2 && $0 ~ /^(pub(\([a-z]+\))? )?mod [A-Za-z_0-9]+ *\{ *$/) skip = 1
  pend = 0; next
}
/^#\[cfg\(test\)\]/ { pend = 2; next }
/^[ \t]+#\[cfg\(test\)\]/ { pend = 1; next }
{ print FILENAME ":" FNR ": " $0 }
