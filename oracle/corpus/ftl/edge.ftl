# comment without a blank line after it
first = First

broken = { $x
ref = { first }
empty =

braces = Use { "{" } and { "}" } literally
multi =
    Line one
      indented more
    last { $v }

shared = { $gender ->
    [male] { $n ->
        [one] He shared one photo.
       *[other] He shared { $n } photos.
    }
   *[other] They shared photos.
}

place = { NUMBER($n, type: "ordinal") ->
    [one] { $n }st
   *[other] { $n }th
}

num = { $n ->
    [0] none
    [1] one
   *[other] many
}

-term = Term value
    .case = nominative

only-attrs =
    .placeholder = Search the web
    .aria-label = Search

trailing = Spaces after   
last = Last one