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
