# Code Review Guidelines

This document establishes code review standards and best practices for the Blixard project to maintain high code quality and consistency.

## Review Principles

### Quality Standards
- **Correctness**: Code must be functionally correct and handle edge cases
- **Safety**: No unsafe patterns, proper error handling, minimal unwrap() usage
- **Performance**: Consider performance implications, especially in critical paths
- **Maintainability**: Code should be readable, well-documented, and modular
- **Security**: Follow security best practices, validate inputs, handle secrets properly

### Review Scope
- **All production code** requires review before merging
- **Test code** should be reviewed for correctness and coverage
- **Documentation changes** need review for accuracy and clarity
- **Configuration changes** require careful review for production impact

## Code Quality Checklist

### Rust-Specific Standards
- [ ] **Error Handling**: Uses `Result` types with proper error propagation
- [ ] **Memory Safety**: No unsafe blocks without clear justification
- [ ] **Borrowing**: Efficient borrowing patterns, minimal clones in hot paths
- [ ] **Async/Await**: Proper async patterns, no blocking in async contexts
- [ ] **Pattern Matching**: Exhaustive pattern matching, no unreachable code

### Architecture & Design
- [ ] **Single Responsibility**: Functions/modules have clear, focused purposes
- [ ] **Separation of Concerns**: Clean boundaries between components
- [ ] **Interface Design**: APIs are intuitive and well-documented
- [ ] **Dependency Management**: Minimal coupling, clear dependency direction
- [ ] **Testability**: Code structure supports effective testing

### Performance Considerations
- [ ] **Hot Path Optimization**: Critical paths are optimized for performance
- [ ] **Resource Management**: Proper cleanup, no resource leaks
- [ ] **Algorithmic Complexity**: Appropriate algorithms for scale requirements
- [ ] **Serialization**: Efficient serialization for network/storage operations
- [ ] **Concurrent Access**: Thread-safe patterns, minimal lock contention

## Security Review Requirements

### Input Validation
- [ ] **Network Inputs**: All network requests validated and sanitized
- [ ] **Configuration**: Config values validated for security implications
- [ ] **User Data**: User-provided data properly escaped and validated
- [ ] **File Operations**: Path traversal and injection prevented

### Authentication & Authorization
- [ ] **Access Controls**: Proper permission checks before operations
- [ ] **Credential Handling**: Secrets never logged or exposed
- [ ] **Session Management**: Secure token handling and expiration
- [ ] **Audit Logging**: Security events properly logged

### Cryptographic Standards
- [ ] **Encryption**: Strong encryption standards (AES-256, Ed25519)
- [ ] **Key Management**: Proper key generation, rotation, and storage
- [ ] **Random Generation**: Cryptographically secure randomness
- [ ] **Certificate Validation**: Proper cert chain validation

## Testing Requirements

### Test Coverage
- [ ] **Unit Tests**: Core functionality has comprehensive unit tests
- [ ] **Integration Tests**: Component interactions tested
- [ ] **Property Tests**: Invariants verified with PropTest where applicable
- [ ] **Error Cases**: Error conditions and edge cases covered

### Test Quality
- [ ] **Deterministic**: Tests are reliable and repeatable
- [ ] **Isolated**: Tests don't depend on external state or other tests
- [ ] **Meaningful**: Tests verify actual behavior, not just completion
- [ ] **Performance**: Critical paths have performance regression tests

### Distributed System Testing
- [ ] **Consensus Testing**: Raft properties verified in various scenarios
- [ ] **Partition Testing**: Network partition recovery tested
- [ ] **Byzantine Testing**: Malicious node behavior handled correctly
- [ ] **Simulation**: MadSim tests for deterministic distributed scenarios

## Documentation Standards

### Code Documentation
- [ ] **Public APIs**: All public functions/types have doc comments
- [ ] **Complex Logic**: Non-obvious code explained with inline comments
- [ ] **Examples**: Usage examples for non-trivial APIs
- [ ] **Error Conditions**: Documented when functions can fail and why

### Architecture Documentation
- [ ] **Design Decisions**: Rationale for architectural choices documented
- [ ] **Trade-offs**: Performance/security/maintainability trade-offs explained
- [ ] **Dependencies**: External dependencies and their purposes documented
- [ ] **Deployment**: Production deployment considerations documented

## Review Process

### Pre-Review Checklist (Author)
- [ ] **Self-Review**: Author has reviewed their own changes thoroughly
- [ ] **Tests**: All tests pass, including new tests for changes
- [ ] **Formatting**: Code formatted with `cargo fmt`
- [ ] **Linting**: No clippy warnings (or explicitly allowed with justification)
- [ ] **Documentation**: Updates to documentation as needed

### Review Guidelines (Reviewer)
- [ ] **Understand Context**: Review the issue/requirement being addressed
- [ ] **Test the Changes**: Verify changes work as intended
- [ ] **Consider Alternatives**: Suggest improvements or alternative approaches
- [ ] **Check Integration**: Ensure changes integrate well with existing code
- [ ] **Future Maintenance**: Consider long-term maintainability implications

### Review Categories

#### Minor Changes (1 reviewer required)
- Bug fixes with clear root cause and solution
- Documentation updates
- Test improvements
- Non-functional refactoring

#### Major Changes (2+ reviewers required)
- New features or significant functionality
- API changes or breaking changes
- Performance optimizations
- Security-related changes
- Database schema changes

#### Critical Changes (Architecture review required)
- Core consensus algorithm changes
- Security model modifications
- Major architectural changes
- External dependency updates

## Common Anti-Patterns to Avoid

### Code Smells
- **Long Functions**: Functions should be focused and reasonably sized
- **Deep Nesting**: Prefer early returns and guard clauses
- **Magic Numbers**: Use named constants for important values
- **Duplicate Code**: Extract common functionality to shared modules
- **Overly Complex Types**: Keep type hierarchies manageable

### Performance Anti-Patterns
- **Unnecessary Cloning**: Use borrowing where possible
- **Blocking in Async**: No blocking operations in async contexts
- **Inefficient Collections**: Use appropriate data structures for access patterns
- **Memory Leaks**: Ensure proper resource cleanup
- **Lock Contention**: Design to minimize lock conflicts

### Security Anti-Patterns
- **Hardcoded Secrets**: Use proper secret management
- **SQL Injection**: Use parameterized queries (if applicable)
- **Path Traversal**: Validate and sanitize file paths
- **Information Disclosure**: Avoid exposing sensitive data in logs/errors
- **Insufficient Validation**: Validate all inputs thoroughly

## Review Response Guidelines

### For Authors
- **Be Receptive**: Accept feedback constructively
- **Ask Questions**: Clarify unclear feedback
- **Explain Decisions**: Provide context for design choices when questioned
- **Address All Feedback**: Respond to every comment appropriately
- **Update Documentation**: Keep docs in sync with code changes

### For Reviewers
- **Be Constructive**: Focus on improving the code, not criticizing the author
- **Explain Reasoning**: Provide context for suggestions
- **Distinguish Issues**: Separate blocking issues from suggestions
- **Acknowledge Good Work**: Recognize well-written code and good practices
- **Suggest Solutions**: Don't just point out problems, suggest improvements

## Approval Criteria

### Required for Approval
- [ ] All reviewer feedback addressed or acknowledged
- [ ] All automated checks passing (tests, lints, formatting)
- [ ] Security review completed for security-sensitive changes
- [ ] Performance impact assessed for performance-critical changes
- [ ] Documentation updated as needed

### Blocking Issues
- Functional incorrectness or bugs
- Security vulnerabilities
- Significant performance regressions
- Breaking API changes without proper deprecation
- Missing tests for new functionality
- Inadequate error handling

### Non-Blocking Issues (Address in follow-up)
- Minor style inconsistencies
- Documentation improvements
- Performance optimizations (unless critical)
- Additional test coverage (if existing coverage is adequate)
- Refactoring suggestions (unless impacting functionality)

## Tools and Automation

### Required Checks
- **Formatting**: `cargo fmt --check`
- **Linting**: `cargo clippy --all-targets --all-features`
- **Testing**: `cargo test --all-features`
- **Documentation**: `cargo doc --no-deps`
- **Security**: Regular dependency audits with `cargo audit`

### Recommended Tools
- **IDE Integration**: Rust Analyzer for real-time feedback
- **Pre-commit Hooks**: Automated formatting and basic checks
- **Coverage Tools**: Track test coverage trends
- **Profiling**: Regular performance profiling for critical paths

---

Following these guidelines ensures consistent, high-quality code that meets enterprise standards and maintains the long-term health of the Blixard codebase.