<?php

declare(strict_types=1);

// Authentication language lines
return [
    /*
     | Used during authentication.
     */
    'failed' => 'These credentials do not match our records.',
    'password' => "The provided password is incorrect.\n",
    'throttle' => 'Too many login attempts. Please try again in :seconds seconds.',
    'welcome' => 'Welcome, :NAME! You have <strong>:count</strong> messages.',
    'apples' => '{0} There are none|[1,19] There are some|[20,*] There are many',
    'simple_plural' => 'apple|apples',
    "it's" => 'It\'s a C:\\path value',
    'nested' => [
        'deep' => [
            'title' => 'Deep title',
            'concat' => 'First half, ' . "second half",
        ],
        'list' => ['One', 'Two', '---', 'Three'],
    ],
    'legacy' => array('ok' => 'Fine', 5 => 'Five', 'Six'),
    'count' => 42,
    'flag' => true,
    'none' => null,
    'empty' => '',
    'symbols' => '...',
];
