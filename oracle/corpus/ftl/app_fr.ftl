### Lumen Bakery (fr)

# Shown on the login button
login = Se connecter
    .title = Cliquez pour vous connecter
    .accesskey = L

## Shop

-brand-name = Boulangerie Lumen
    .gender = feminine

welcome = { $name }, { -brand-name }: Du pain frais chaque matin

loaves = { $count ->
    [one] { $count } un pain
   *[other] { $count } pains
}

tagline =
    Du pain frais chaque matin
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
