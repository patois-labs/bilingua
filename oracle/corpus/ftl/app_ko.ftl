### Lumen Bakery (ko)

# Shown on the login button
login = 로그인
    .title = 로그인하려면 클릭
    .accesskey = L

## Shop

-brand-name = 루멘 베이커리
    .gender = feminine

welcome = { $name }, { -brand-name }: 매일 아침 신선한 빵

loaves = { $count ->
    [one] { $count } 빵 한 개
   *[other] { $count } 개의 빵
}

tagline =
    매일 아침 신선한 빵
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
