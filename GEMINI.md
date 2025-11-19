# VT Code - Project Overview

This document provides a comprehensive overview of the `vtcode` project, designed to serve as instructional context for the Gemini CLI agent.

## Project Overview

**VT Code** is a Rust-based terminal coding agent that integrates semantic code intelligence via Tree-sitter. It supports multiple Large Language Model (LLM) providers, offering features like automatic failover and efficient context management. The project aims to enhance developer workflows through smart tools, editor integrations (including Zed and Visual Studio Code), and a robust, multi-layered security model.

**Key Features:**
*   **Multi-Provider AI:** Compatibility with various LLM providers (OpenAI, Anthropic, Google Gemini, xAI, DeepSeek, OpenRouter, Z.AI, Moonshot AI, MiniMax, Ollama, LM Studio).
*   **Code Intelligence:** Utilizes Tree-sitter parsers for multiple languages (Rust, Python, JavaScript/TypeScript, Go, Java, Swift).
*   **Smart Tools:** Built-in capabilities for code analysis, file operations, terminal commands, and refactoring.
*   **Editor Integration:** Native support for Zed IDE via Agent Client Protocol (ACP) and a Visual Studio Code extension.
*   **Lifecycle Hooks:** Allows execution of custom shell commands in response to agent events for context enrichment, policy enforcement, and automation.
*   **Context Management:** Advanced token budget tracking and dynamic context curation.
*   **TUI Interface:** Provides a rich terminal user interface with real-time streaming.

## Building and Running

The project is a Rust workspace composed of several crates.

### Installation

*   **macOS & Linux:**
    ```bash
    curl -fsSL https://raw.githubusercontent.com/vinhnx/vtcode/main/scripts/install.sh | bash
    ```
*   **Windows (PowerShell):**
    ```powershell
    irm https://raw.githubusercontent.com/vinhnx/vtcode/main/scripts/install.ps1 | iex
    ```
*   **Package Managers:**
    *   Homebrew: `brew install vtcode`
    *   Cargo: `cargo install vtcode`
    *   npm: `npm install -g @vinhnx/vtcode`

### Setup and Launch

1.  **Set your API key** for your chosen LLM provider (e.g., for OpenAI):
    ```bash
    export OPENAI_API_KEY="sk-..."
    ```
    (Refer to `docs/installation/#supported-ai-providers` for other providers.)
2.  **Launch VT Code:**
    ```bash
    vtcode
    ```

### Development Commands

*   **Build:**
    *   Standard build: `cargo build`
    *   Release build: `cargo build --release`
*   **Run:**
    *   Execute the main `vtcode` binary: `cargo run`
*   **Test:**
    *   Run all tests: `cargo test`
    *   The `scripts/check.sh` script likely performs comprehensive checks including tests and linting.

## Development Conventions

*   **Language:** Rust (edition 2024).
*   **Toolchain:** Managed by `rust-toolchain.toml`, specifying `channel = "1.90.0"` and components like `clippy` and `rustfmt`.
*   **Formatting:** Code formatting is enforced using `rustfmt`, configured via `rustfmt.toml`.
*   **Linting:** Extensive `clippy` lints are configured in `Cargo.toml` to maintain high code quality and prevent common Rust pitfalls.
*   **Dependencies:** Managed through `Cargo.toml` for the main project and individual crates within the workspace.
*   **Workspace Structure:** The project is organized as a Cargo workspace, including crates such as `vtcode-acp-client`, `vtcode-core`, `vtcode-commons`, `vtcode-config`, `vtcode-llm`, `vtcode-markdown-store`, `vtcode-indexer`, `vtcode-tools`, `vtcode-bash-runner`, and `vtcode-exec-events`.

## Security & Safety

VT Code implements a defense-in-depth security model to protect against prompt injection and argument injection attacks, including:
*   **Execution Policy:** Command allowlist with per-command argument validation.
*   **Workspace Isolation:** All operations confined to workspace boundaries.
*   **Sandbox Integration:** Optional Anthropic sandbox runtime for network commands.
*   **Human-in-the-Loop:** Configurable approval system for sensitive operations.
*   **Audit Trail:** Comprehensive logging of all command executions.
(Refer to `docs/SECURITY_MODEL.md` for more details.)

---

## Recommended Improvements To-Do List

This section outlines a detailed plan to implement the recommended improvements for the `vtcode` terminal application.

### Phase 1: Build Acceleration & Foundational Improvements

1.  **Optimize Compilation Framework with Build Acceleration Tools:**
    *   **Subtask 1.1:** Investigate and configure `sccache` for Rust builds.
        *   Install `sccache`.
        *   Set `RUSTC_WRAPPER=sccache` in the environment.
        *   Verify `sccache` is working by checking cache hits/misses.
    *   **Subtask 1.2:** Investigate `ninja` or `ccache` for potential integration (less direct for Cargo, but explore if applicable for C/C++ dependencies if any).
    *   **Subtask 1.3:** Evaluate `vcpkg` for managing C/C++ dependencies if the project introduces any, ensuring consistent and accelerated builds.
    *   **Subtask 1.4:** Document the setup and usage of these tools in `GEMINI.md` or a dedicated `BUILD_GUIDE.md`.

### Phase 2: Performance Improvements

2.  **Enhance Existing Prompt Cache:**
    *   **Subtask 2.1:** Audit the current `PromptCache` (already implemented) for hit-rate stats and eviction behavior.
    *   **Subtask 2.2:** Expose cache metrics (hits, misses, bytes) to telemetry/logging.
    *   **Subtask 2.3:** Add config flags for cache warmup/priming and quality-threshold tuning.
    *   **Subtask 2.4:** Surface cache controls (clear/show stats) in the TUI/CLI.
3.  **Optimized Context Pruning Strategies:**
    *   **Subtask 3.1:** Research advanced context pruning techniques (e.g., AST-based relevance, code graph analysis).
    *   **Subtask 3.2:** Integrate Tree-sitter output or a code graph representation into the context management module (`vtcode-core`).
    *   **Subtask 3.3:** Implement a new pruning algorithm that prioritizes code snippets based on their semantic relevance to the current task/prompt.
    *   **Subtask 3.4:** Add configuration options to select different pruning strategies.
4.  **Asynchronous Tool Execution with Progress Feedback:**
    *   **Subtask 4.1:** Review existing tool execution mechanisms (`vtcode-tools`, `vtcode-bash-runner`).
    *   **Subtask 4.2:** Ensure all external commands are executed asynchronously using `tokio::spawn` or similar.
    *   **Subtask 4.3:** Implement a standardized way for tools to report progress (e.g., via channels or event streams).
    *   **Subtask 4.4:** Integrate progress reporting into the TUI to display real-time feedback to the user.
5.  **Extend `vtcode-indexer` (already present):**
    *   **Subtask 5.1:** Add incremental/index-delta mode instead of full rescans.
    *   **Subtask 5.2:** Persist symbol/refs/call-graph overlays using Tree-sitter outputs.
    *   **Subtask 5.3:** Optimize storage layout (e.g., mmap/SQLite) and size reporting.
    *   **Subtask 5.4:** Wire indexed data into `find_symbol` / `find_referencing_symbols` for low-latency queries.
    *   **Subtask 5.5:** Make indexing frequency/scope configurable per workspace.
6.  **Lazy Loading of Large File Contents:**
    *   **Subtask 6.1:** Identify areas in the TUI where large file contents are displayed (e.g., search results, file viewer).
    *   **Subtask 6.2:** Implement a mechanism to load file content in chunks or on demand as the user scrolls.
    *   **Subtask 6.3:** Update the TUI components to handle partial content and display loading indicators for unseen parts.
7.  **Efficient Diffing for UI Updates:**
    *   **Subtask 7.1:** Research TUI rendering libraries or custom diffing algorithms suitable for `ratatui`.
    *   **Subtask 7.2:** Implement a diffing layer that compares the previous and current UI state.
    *   **Subtask 7.3:** Apply updates only to the changed regions of the terminal, minimizing re-renders.

### Phase 3: UX/UI Improvements

8.  **Interactive Command History and Autocompletion:**
    *   **Subtask 8.1:** Implement a persistent command history.
    *   **Subtask 8.2:** Develop an autocompletion engine for `vtcode` commands, arguments, and file paths.
    *   **Subtask 8.3:** Integrate fuzzy matching and contextual suggestions into the autocompletion.
    *   **Subtask 8.4:** Enhance the input field in the TUI to support interactive history navigation (e.g., Up/Down arrows) and autocompletion display.
9.  **Configurable TUI Layouts and Themes:**
    *   **Subtask 9.1:** Define a configuration schema for TUI layouts (e.g., pane arrangements, sizes).
    *   **Subtask 9.2:** Implement a theme system allowing users to define color palettes and styling.
    *   **Subtask 9.3:** Create default layouts and themes, and allow users to switch between them via configuration or commands.
    *   **Subtask 9.4:** Update the TUI rendering logic to apply selected layouts and themes dynamically.
10. **Rich Error Reporting with Actionable Suggestions:**
    *   **Subtask 10.1:** Centralize error handling in `vtcode-core`.
    *   **Subtask 10.2:** Map common errors to user-friendly messages and actionable suggestions.
    *   **Subtask 10.3:** Display enhanced error messages prominently in the TUI, potentially with links to relevant documentation or commands.
11. **Visual Indicators for Agent State:**
    *   **Subtask 11.1:** Define a set of distinct agent states (e.g., idle, thinking, executing, streaming, waiting).
    *   **Subtask 11.2:** Implement a state management system that updates the current agent state.
    *   **Subtask 11.3:** Design and integrate visual indicators (e.g., status bar messages, animated spinners, color changes) into the TUI to reflect the agent's state.
12. **Integrated Code Snippet Management:**
    *   **Subtask 12.1:** Design a system to store and retrieve code snippets (e.g., local file storage, simple database).
    *   **Subtask 12.2:** Implement commands to save, list, and insert snippets.
    *   **Subtask 12.3:** Integrate snippet management into the TUI, allowing users to browse and select snippets.
    *   **Subtask 12.4:** Consider basic versioning or tagging for snippets.
13. **Guided Workflow for Complex Tasks:**
    *   **Subtask 13.1:** Identify a few common complex tasks (e.g., "add a new Rust module").
    *   **Subtask 13.2:** Develop a "workflow engine" that can guide the user through predefined steps.
    *   **Subtask 13.3:** Implement TUI components to display workflow progress, current step, and prompt for user input.
    *   **Subtask 13.4:** Integrate this with existing tools and LLM capabilities to automate parts of the workflow.
14. **Contextual Help and Documentation Access:**
    *   **Subtask 14.1:** Create a searchable index of `vtcode` commands, configuration options, and documentation topics.
    *   **Subtask 14.2:** Implement a `help` command that provides contextual information based on the current input or agent state.
    *   **Subtask 14.3:** Allow users to quickly open relevant documentation files in their default browser from within the TUI.

### Phase 4: Operational Factors

15. **Robust Plugin/Extension System:**
    *   **Subtask 15.1:** Define a clear API and lifecycle for `vtcode` plugins/extensions.
    *   **Subtask 15.2:** Implement a plugin loader that can discover and load external plugins.
    *   **Subtask 15.3:** Provide documentation and examples for developing new plugins (e.g., custom tools, LLM providers).
    *   **Subtask 15.4:** Consider a plugin registry or marketplace.
16. **Centralized Configuration Management with Validation:**
    *   **Subtask 16.1:** Enhance the `vtcode-config` crate with a robust configuration schema definition.
    *   **Subtask 16.2:** Implement strict validation for `vtcode.toml` settings, providing clear error messages for invalid configurations.
    *   **Subtask 16.3:** Develop an interactive TUI-based configuration wizard for initial setup and guided modifications.
17. **Improved Observability and Debugging Tools:**
    *   **Subtask 17.1:** Enhance logging in `vtcode-core` and other crates to include structured logs (e.g., JSON format).
    *   **Subtask 17.2:** Implement a debug mode that provides detailed insights into agent decisions, LLM prompts, and tool outputs.
    *   **Subtask 17.3:** Develop a TUI-based log viewer or integrate with external logging tools.
18. **Cross-Platform Binary Distribution and Auto-Updates:**
    *   **Subtask 18.1:** Automate the build and packaging process for macOS, Linux, and Windows.
    *   **Subtask 18.2:** Integrate an auto-update mechanism (e.g., using `self_update` crate or similar) into the `vtcode` binary.
    *   **Subtask 18.3:** Ensure smooth update experience with rollback capabilities.
19. **Git Staging/Commit UX (status/diff already implemented):**
    *   **Subtask 19.1:** Integrate `git2-rs` (or shell fallback) to stage files and create commits from the TUI.
    *   **Subtask 19.2:** Add visual staging cues and inline diffs for selected files.
    *   **Subtask 19.3:** Suggest commit messages based on agent-made changes and user context.
    *   **Subtask 19.4:** Support basic rollback (reset selected files) from the UI.
20. **Telemetry Opt-in for Usage Analytics:**
    *   **Subtask 20.1:** Implement an *opt-in* mechanism for telemetry, clearly explaining what data is collected.
    *   **Subtask 20.2:** Integrate a lightweight analytics library to collect anonymous usage data (e.g., command usage, feature adoption).
    *   **Subtask 20.3:** Ensure compliance with privacy regulations and provide clear opt-out instructions.
