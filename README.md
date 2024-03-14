# cargo-context-ranger

## Overview

`cargo-context-ranger` is a Rust developer's tool for enriching the debugging and development process with the power of Large Language Models (LLM). By extracting and prioritizing context from Rust codebases, it prepares detailed prompts for LLM queries, focusing on a specific function and its interactions within the project. This targeted approach enables developers to gain insights and assistance tailored to the exact scope of their work.

### Key Features

- **Function-Centric Context Extraction:** Gathers context starting from a specific function, optimizing for relevance in LLM queries.
- **Modular Window Size:** Configurable context size in kilo characters (K chars), allowing for adjustable breadth in the extracted text.
- **Comprehensive Context Compilation:** Aggregates essential project details, including platform, compiler version, project dependencies, and more, to form a complete picture for the LLM.
- **Streamlined Prompt Preparation:** Facilitates the creation of rich, informative prompts for LLM, reducing manual effort and improving query effectiveness.

## Getting Started

### Prerequisites

Before you begin, make sure Rust and Cargo are installed on your system. If you need to install these, follow the setup instructions at [https://rustup.rs/](https://rustup.rs/).

### Installation

Install `cargo-context-ranger` directly from crates.io using Cargo. Simply execute the following command:

```bash
cargo install cargo-context-ranger
```

This command fetches and installs `cargo-context-ranger` along with all necessary dependencies.

### Usage

With `cargo-context-ranger` installed, you can invoke it as follows:

```bash
cargo-context-ranger -p <path_to_rust_project> -f <full_module_path_to_function> [-w <window_size_in_k_chars>]
```

#### Parameters:

- `-p, --path`: Path to the Rust project directory.
- `-f, --function`: Full module path to the target function for analysis.

#### Optional Parameter:

- `-w, --window`: Window size in kilo characters (K chars) to limit the output. Defaults to 16K if not specified.

Example command:

```bash
cargo-context-ranger -p /path/to/myproject -f src/lib::my_module::my_function -w 500
```

## How It Works

`cargo-context-ranger` operates by:

1. Identifying the specified function within the Rust project.
2. Extracting the target function's body, relevant calling or called functions, and any directly related unit tests.
3. Ordering the content from most to least relevant, using "..." for conciseness where necessary.
4. Optionally trimming the output to the specified window size, focusing on the most relevant text for LLM inquiry.

The output includes detailed project and environment context, such as:

- Rust compiler version
- Operating system
- Project structure and dependencies
- Development environment specifics
- And more, providing a rich backdrop for LLM prompts.

## License

`cargo-context-ranger` is made available under the MIT License. For full license text, refer to the LICENSE file in the project repository.




