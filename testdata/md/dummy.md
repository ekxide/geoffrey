# Dummy Documentation

All glory Hypnotoad!

<!-- [geoffrey] [testdata/content/dummy.hpp] [[the question] [response]] -->
```cpp
uint8_t answerToTheUniverseAndEverything() {
    // ...
    return ANSWER;
}
```

<!-- [geoffrey] [testdata/content/dummy.hpp] [the answer] -->
```cpp
constexpr uint8_t ANSWER{42U};
```


Entire files can be embedded
<!-- [geoffrey] [testdata/content/goat.txt] -->
```
  -^-_-^-_-^-_-^-_-^-_-^-
 (                       )
(   all glory hypnotoad   )
 (                       )
  -_-^-_-^-_-^-_-^-_-^-_- _
                         (_)
                            o
                              /_/
                             (oo)-------/
                             (_)-(      )
                               v  ||--||
                                  ||  ||
```


A manually managed code block
```cpp
auto foo = "bar";
```

Let's include the `main.cpp`
<!-- [geoffrey] [testdata/content/main.cpp] -->
```cpp
#include <iostream>

int main() {

    constexpr uint64_t ANSWER {42};

    for(uint64_t i = 0; i < ANSWER; ++i) {
        std::cout << i << " is not the answer"<< std::endl;
    }

    std::cout << "it's " << ANSWER << std::endl;

    return EXIT_SUCCESS;
}
```

Only the main function
<!-- [geoffrey] [testdata/content/main.cpp] [main function] -->
```cpp
int main() {

    constexpr uint64_t ANSWER {42};

    for(uint64_t i = 0; i < ANSWER; ++i) {
        std::cout << i << " is not the answer"<< std::endl;
    }

    std::cout << "it's " << ANSWER << std::endl;

    return EXIT_SUCCESS;
}
```

Only the main function with elided nested snippets
<!-- [geoffrey] [testdata/content/main.cpp] [[main function]] -->
```cpp
int main() {
    // ...
    return EXIT_SUCCESS;
}
```

Only the main function with elided nested snippets
<!-- [geoffrey] [testdata/content/main.cpp] [[main function] [define answer] [print answer]] -->
```cpp
int main() {

    constexpr uint64_t ANSWER {42};
    // ...
    std::cout << "it's " << ANSWER << std::endl;

    return EXIT_SUCCESS;
}
```
