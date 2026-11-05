### Lumen Bakery (en)

# Shown on the login button
login = Log in
    .title = Click to log in
    .accesskey = L

## Shop

-brand-name = Lumen Bakery
    .gender = feminine

welcome = { $name }, { -brand-name }: Fresh bread every morning

loaves = { $count ->
    [one] { $count } one loaf
   *[other] { $count } loaves
}

tagline =
    Fresh bread every morning
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
