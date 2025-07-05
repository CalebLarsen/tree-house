   use std::collections;
// ┡━┛ ┡━┛┡┛┡━━━━━━━━━┛╰─ punctuation.delimiter
// │   │  │ ╰─ namespace
// │   │  ╰─ punctuation.delimiter
// │   ╰─ namespace
// ╰─ keyword.control.import
   fn fun<T>(collections: u32, v: collections::VecDeque<T>) -> u32 {
// ┡┛ ┡━┛╿╿┡┛┡━━━━━━━━━┛╿ ┡━┛╿ ╿╿ ┡━━━━━━━━━┛┡┛┡━━━━━━┛╿╿┡┛ ┡┛ ┡━┛ ╰─ punctuation.bracket
// │  │  │││ │          │ │  │ ││ │          │ │       │││  │  ╰─ type.builtin
// │  │  │││ │          │ │  │ ││ │          │ │       │││  ╰─ operator
// │  │  │││ │          │ │  │ ││ │          │ │       ││╰─ punctuation.bracket
// │  │  │││ │          │ │  │ ││ │          │ │       │╰─ type
// │  │  │││ │          │ │  │ ││ │          │ │       ╰─ punctuation.bracket
// │  │  │││ │          │ │  │ ││ │          │ ╰─ type
// │  │  │││ │          │ │  │ ││ │          ╰─ punctuation.delimiter
// │  │  │││ │          │ │  │ ││ ╰─ namespace
// │  │  │││ │          │ │  │ │╰─ punctuation.delimiter
// │  │  │││ │          │ │  │ ╰─ variable.parameter
// │  │  │││ │          │ │  ╰─ punctuation.delimiter
// │  │  │││ │          │ ╰─ type.builtin
// │  │  │││ │          ╰─ punctuation.delimiter
// │  │  │││ ╰─ variable.parameter
// │  │  ││╰─ punctuation.bracket
// │  │  │╰─ type.parameter
// │  │  ╰─ punctuation.bracket
// │  ╰─ function
// ╰─ keyword.function
    let _ = collections::VecDeque::new();
//  ┡━┛   ╿ ┡━━━━━━━━━┛┡┛┡━━━━━━┛┡┛┡━┛┡┛╰─ punctuation.delimiter
//  │     │ │          │ │       │ │  ╰─ punctuation.bracket
//  │     │ │          │ │       │ ╰─ function
//  │     │ │          │ │       ╰─ punctuation.delimiter
//  │     │ │          │ ╰─ type
//  │     │ │          ╰─ punctuation.delimiter
//  │     │ ╰─ namespace
//  │     ╰─ operator
//  ╰─ keyword.storage
    let colly = 1;
//  ┡━┛ ┡━━━┛ ╿ ╿╰─ punctuation.delimiter
//  │   │     │ ╰─ constant.numeric.integer
//  │   │     ╰─ operator
//  │   ╰─ variable
//  ╰─ keyword.storage
    if colly == v.len() {
//  ┡┛ ┡━━━┛ ┡┛ ╿╿┡━┛┡┛ ╰─ punctuation.bracket
//  │  │     │  │││  ╰─ punctuation.bracket
//  │  │     │  ││╰─ function
//  │  │     │  │╰─ punctuation.delimiter
//  │  │     │  ╰─ variable.parameter
//  │  │     ╰─ operator
//  │  ╰─ variable
//  ╰─ keyword.control.conditional
        return collections + 1;
//      ┡━━━━┛ ┡━━━━━━━━━┛ ╿ ╿╰─ punctuation.delimiter
//      │      │           │ ╰─ constant.numeric.integer
//      │      │           ╰─ operator
//      │      ╰─ variable.parameter
//      ╰─ keyword.control.return
    }
//  ╰─ punctuation.bracket
    collections
//  ┗━━━━━━━━━┹─ variable.parameter
   }
// ╰─ punctuation.bracket
