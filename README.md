# geoffrey - Syncs Source Code To Markdown Code Blocks

`geoffrey` is a small tool to help to keep the code blocks in a markdown file in sync with source files.

This is done by using doxygen snippet annotations in the source files and geoffrey tags in the markdown file.

## Usage

A geoffrey tag is wrapped into a comment and consists of at least two segments which are embedded in `[]` brackets.
The first segment is always `[geoffrey]`, which is used to unambiguously identify a geoffrey tag.
The second segment is the path to the source file. The path must be relative to the git top-level directory.
With these two segments, the whole source file will be inserted in the markdown code block.
In order to insert only one snippet, a third segment with the name of the doxygen snippet has to be supplied.

Right after the geoffrey tag a markdown code block must follow. This is the place where the snippets are inserted.
The doxygen snippet names will be remove before the code is inserted into the markdown file.

For a whole file
`````
<!-- [geoffrey] [path/to/source/file] -->
```cpp
```
`````

For snippet only, including nested snippets
`````
<!-- [geoffrey] [path/to/source/file] [snippet name] -->
```cpp
```
`````

For snippet with elided nested snippets
`````
<!-- [geoffrey] [path/to/source/file] [[snippet mane]] -->
```cpp
```
`````

For snippet with some nested snippets not elided
`````
<!-- [geoffrey] [path/to/source/file] [[snippet name] [name of not elided snippet 1] [name of not elided snippet 2]] -->
```cpp
```
`````

When geoffrey is invoked, a path to the directory with the markdown files or a single markdown file must be passed as cmd line argument
```sh
geoffrey doc
```
or
```sh
geoffrey doc/README.md
```

Subsequent runs of geoffrey will update the code blocks with the content from the source files.

## Example

Let's assume you have the following C++ source file
```cpp
//! [includes]
#include <iostream>
//! [includes]

//! [main function]
int main() {

    //! [define answer]
    constexpr uint64_t ANSWER {42};
    //! [define answer]

    //! [print till answer]
    for(uint64_t i = 0; i < ANSWER; ++i) {
        std::cout << i << " is not the answer"<< std::endl;
    }
    //! [print till answer]

    //! [print answer]
    std::cout << "it's " << ANSWER << std::endl;
    //! [print answer]

    return EXIT_SUCCESS;
}
//! [main function]
```

As you can see, the file has doxygen snippet tags all over the place.

### Whole File

Assuming the `main.cpp` is in the `source` folder
`````
<!-- [geoffrey] [source/main.cpp] -->
```cpp
```
`````

After running geoffrey, the markdown file will have this code
`````cpp
<!-- [geoffrey] [source/main.cpp] -->
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
`````


### Snippet Only (Including Nested Snippets)

This
`````
<!-- [geoffrey] [source/main.cpp] [main function] -->
```cpp
```
`````

becomes this
`````cpp
<!-- [geoffrey] [source/main.cpp] [main function] -->
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
`````

### Snippet With Elided Nested Snippets

This
`````
<!-- [geoffrey] [source/main.cpp] [main function] -->
```cpp
```
`````

becomes this
`````cpp
<!-- [geoffrey] [source/main.cpp] [[main function]] -->
```cpp
int main() {
    // ...
    return EXIT_SUCCESS;
}
```
`````

### Snippet With Some Nested Snippets Not Elided

This
`````
<!-- [geoffrey] [source/main.cpp] [[main function] [define answer] [print answer]] -->
```cpp
```
`````

becomes this
`````cpp
<!-- [geoffrey] [source/main.cpp] [[main function] [define answer] [print answer]] -->
```cpp
int main() {

    constexpr uint64_t ANSWER {42};
    // ...
    std::cout << "it's " << ANSWER << std::endl;

    return EXIT_SUCCESS;
}
```
`````
