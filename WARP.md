# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Essential Commands

### Build & Run
```bash
# Quick compilation check (preferred)
cargo check

# Release build (production)
cargo build --release

# Debug build (faster compilation)
cargo build

# Run in release mode
scripts/run.sh

# Run in debug mode (faster build, supports .env file)
scripts/run-debug.sh

# Single query (headless testing)
cargo run -- ask "your query"
```

### Testing
```bash
# Run all tests (preferred - uses nextest)
cargo nextest run

# Run all tests (standard)
cargo test

# Run specific test
cargo nextest run test_name
# or
cargo test test_name

# Run with debug output
cargo test -- --nocapture
```

### Code Quality
```bash
# Linting (must pass before commit)
cargo clippy

# Formatting
cargo fmt

# Check formatting without modifying
cargo fmt --check

# Pre-commit checks
cargo clippy && cargo fmt --check && cargo check && cargo nextest run
```

## Architecture Overview

### Workspace Structure
VT Code uses a modular workspace architecture:

- **`vtcode-core/`**: Core library code
  - `llm/`: LLM provider abstraction (OpenAI, Anthropic, Gemini, xAI, DeepSeek, Z.AI, Moonshot AI, OpenRouter, Ollama)
  - `tools/`: Trait-based tool system with modular capabilities
  - `config/`: TOML-based configuration parsing and validation
  - `code/`: Tree-sitter integration for semantic code analysis
  - `mcp/`: Model Context Protocol integration
  - `acp/`: Agent Client Protocol support
  - `sandbox/`: Secure execution environment

- **`src/`**: CLI binary executable
  - Ratatui TUI for rich terminal interface
  - PTY (pseudo-terminal) execution for command streaming
  - Slash commands and interactive features

- **Workspace Crates**:
  - `vtcode-acp-client`: Agent Client Protocol client for editor integration (Zed, etc.)
  - `vtcode-commons`: Shared utilities and types
  - `vtcode-config`: Configuration management (reusable)
  - `vtcode-llm`: LLM provider implementations
  - `vtcode-tools`: Tool implementations and registry
  - `vtcode-bash-runner`: Shell command execution
  - `vtcode-indexer`: Code indexing and search
  - `vtcode-markdown-store`: Markdown-based knowledge storage
  - `vtcode-exec-events`: Execution event handling

### Core Design Principles
1. **Trait-based composability**: Tools implement multiple traits for different capabilities
2. **Mode-based execution**: Single tools support multiple execution modes
3. **Provider abstraction**: Uniform async interfaces across all LLM providers
4. **Modular tools system**: Extensible trait-based architecture (77% complexity reduction)
5. **Security first**: Multi-layered security with execution policies, workspace isolation, and sandbox integration

### Key Technologies
- **Tree-sitter**: Semantic code analysis for Rust, Python, JavaScript/TypeScript, Go, Java, Swift
- **Ratatui**: Terminal user interface with real-time streaming
- **PTY Integration**: Real-time command execution and output streaming
- **MCP**: Model Context Protocol for extensible tooling
- **ACP**: Agent Client Protocol for editor integration (Zed IDE)

## Configuration Patterns

### Critical Rules
1. **Never hardcode values** - Always read from `vtcode.toml` or constants
2. **Model IDs** - Reference `vtcode-core/src/config/constants.rs` for model constants
3. **Model information** - Check `docs/models.json` for current model IDs and capabilities
4. **Documentation** - All .md files must be placed in `./docs/` directory (not root)

### Configuration Files
- `vtcode.toml`: Primary configuration file (user-editable)
- `vtcode.toml.example`: Example configuration with all options
- `vtcode-core/src/config/constants.rs`: Constant definitions (reference in code)
- `docs/models.json`: Model IDs and specifications for all providers

### Example: Referencing Models
```rust
// ✗ Bad - hardcoded
let model = "gemini-2.5-flash";

// ✓ Good - use constants module
use vtcode_core::config::constants::models::google::GEMINI_2_5_FLASH;
let model = GEMINI_2_5_FLASH;
```

## Code Style & Conventions

### Error Handling
Always use `anyhow::Result<T>` with `.with_context()` for descriptive errors:

```rust
use anyhow::{Context, Result};

pub async fn read_file(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))
}
```

### Naming Conventions
- Functions/variables: `snake_case`
- Types: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`

### Formatting
- Use 4 spaces for indentation (no tabs)
- Prefer early returns over nested conditionals
- Use descriptive variable names over complex expressions
- Run `cargo fmt` before committing

### Important Rules
- **No emojis** in code or comments
- **No hardcoded values** - always use configuration
- **Descriptive error messages** with `.with_context()`
- **All documentation** goes in `./docs/` folder only
- **Prefer composition** over deep inheritance
- **Use deep modules**: simple interface, complex functionality

## Development Workflow

### Pre-Commit Checklist
1. Run `cargo clippy` (must pass - enforced linting rules)
2. Run `cargo fmt` (automatic formatting)
3. Run `cargo nextest run` (or `cargo test`)
4. Verify code compiles: `cargo check`
5. Ensure no hardcoded values or model IDs

### Testing Strategy
- **Unit tests**: Co-located with source in `#[cfg(test)]` modules
- **Integration tests**: In `tests/` directory
- **Use nextest**: Preferred over standard `cargo test` for faster, more reliable runs
- **Mock testing**: Use realistic mock data for external dependencies

### Debugging
- Use single prompt mode for headless testing: `cargo run -- ask "query"`
- Enable verbose logging with `RUST_LOG=debug`
- Run with `--nocapture` to see println! output: `cargo test -- --nocapture`
- Use debug build for faster iteration: `scripts/run-debug.sh`

### Adding New Tools
1. Implement `Tool` trait in `vtcode-core/src/tools/`
2. Optionally implement `ModeTool` for multi-mode support
3. Register in `vtcode-core/src/tools/registry.rs`
4. Add tests in the same file or `tests/`
5. Update documentation in `./docs/`

## Important Integration Points

### LLM Providers
Configure via environment variables:
- `OPENAI_API_KEY` - OpenAI GPT models
- `ANTHROPIC_API_KEY` - Anthropic Claude models
- `GEMINI_API_KEY` or `GOOGLE_API_KEY` - Google Gemini models
- `XAI_API_KEY` - xAI Grok models
- `DEEPSEEK_API_KEY` - DeepSeek models
- `ZAI_API_KEY` - Z.AI GLM models
- `MOONSHOT_API_KEY` - Moonshot AI Kimi models
- `OPENROUTER_API_KEY` - OpenRouter (marketplace)
- `OLLAMA_API_KEY` - Ollama (optional, for local models)
- `LMSTUDIO_BASE_URL` - LM Studio (local models)

### Agent Client Protocol (ACP)
VT Code is a fully capable ACP agent that integrates with ACP clients like Zed IDE.
See `docs/guides/zed-acp.md` for configuration details.

### Model Context Protocol (MCP)
VT Code supports MCP for extensible tooling and context management.
See `docs/guides/mcp-integration.md` for configuration and provider setup.

### Tree-sitter Code Analysis
- Supported languages: Rust, Python, JavaScript, TypeScript, Go, Java, Swift
- Efficient parsing with size limits and graceful degradation
- Used for semantic code understanding and symbol extraction

## Security Considerations

VT Code implements defense-in-depth security:

1. **Execution Policy**: Command allowlist with per-command argument validation
2. **Workspace Isolation**: All operations confined to workspace boundaries
3. **Sandbox Integration**: Optional Anthropic sandbox runtime for network commands
4. **Human-in-the-Loop**: Configurable approval system for sensitive operations
5. **Audit Trail**: Comprehensive logging of all command executions

See `docs/SECURITY_MODEL.md` for complete security documentation.

## Common Issues & Solutions

### Build Performance
- Use `cargo check` instead of `cargo build` for quick validation
- Use debug builds (`cargo build`) during development
- Use release builds (`cargo build --release`) for production/testing

### Test Failures
- Ensure API keys are set for provider-dependent tests
- Some tests may require external services (MCP providers, etc.)
- Use `cargo nextest run` for more reliable test execution

### Configuration Issues
- Check `vtcode.toml` for syntax errors (TOML format)
- Verify constants are defined in `vtcode-core/src/config/constants.rs`
- Reference `docs/models.json` for valid model IDs

## Additional Resources

- **Architecture**: `docs/ARCHITECTURE.md`
- **Contributing**: `CONTRIBUTING.md`
- **Security Model**: `docs/SECURITY_MODEL.md`
- **Testing Guide**: `docs/development/testing.md`
- **MCP Integration**: `docs/guides/mcp-integration.md`
- **ACP Integration**: `docs/guides/zed-acp.md`
- **Provider Guides**: `docs/PROVIDER_GUIDES.md`
- **Context Engineering**: `docs/context_engineering.md`

---

For additional help, see the comprehensive documentation in the `docs/` directory or refer to `AGENTS.md` for agent-specific guidelines.
