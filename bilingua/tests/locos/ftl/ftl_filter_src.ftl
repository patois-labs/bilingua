### Resource comment

# Simple message with a variable.
hello = Hello, { $name }!

## Group: brand
-brand-name = Firefox
    .gender = masculine

emails = { $unread ->
    [one] You have one unread email.
   *[other] You have { $unread } unread emails in { -brand-name }.
}

multi =
    First line of the text
    second line { "{" } brace

login-button = Log in
    .title = Click to log in
    .accesskey = L

only-attrs =
    .placeholder = Search the web

numbers = { NUMBER($n, minimumFractionDigits: 2) }
broken = { $x
ref = { hello }
