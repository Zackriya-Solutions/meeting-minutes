# Contributing to PulseTalk

Thank you for contributing to PulseTalk. This document covers the local development workflow. See [Keeping PulseTalk up to date](docs/UPSTREAM_SYNC.md) for the fork and upstream sync process.

## Development Workflow

### Branch Strategy

- `main` - Production branch
- `devtest` - Development and testing branch
- Feature branches should be created from `devtest`

### Getting Started

1. Clone the PulseTalk repository:
   ```bash
   git clone https://github.com/Qblaauw/PulseTalk.git
   ```
2. Add the original Meetily repository as upstream if it is not already configured:
   ```bash
   git remote add upstream https://github.com/Zackriya-Solutions/meetily.git
   ```
3. Create a new branch from the current PulseTalk `main` branch:
   ```bash
   git switch main
   git pull --ff-only origin main
   git switch -c feature/your-feature-name
   ```

### Development Process

1. Always start your work from the current PulseTalk `main` branch
2. Create a new branch for each feature/fix
3. Make your changes
4. Write or update tests as needed
5. Ensure all tests pass
6. Update documentation if necessary

### Issue Creation

Before starting work on a new feature or bug fix:

1. Check if an issue already exists
2. If not, create a new issue with:
   - Clear title
   - Detailed description
   - Steps to reproduce (for bugs)
   - Expected behavior
   - Screenshots (if applicable)
   - Labels (bug, enhancement, etc.)

### Pull Request Process

1. Create a PR from your feature branch to PulseTalk `main`
2. Link the PR to the related issue using the issue number (e.g., "Fixes #123")
3. Fill out the PR template completely
4. Ensure CI checks pass
5. Request review from at least one maintainer
6. Address any review comments
7. Once approved, the PR will be merged into `devtest`

### PR Template

```markdown
## Description
[Describe your changes here]

## Related Issue
[Link to the issue this PR addresses]

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Code refactoring
- [ ] Other (please describe)

## Testing
- [ ] Unit tests added/updated
- [ ] Manual testing performed
- [ ] All tests pass

## Documentation
- [ ] Documentation updated
- [ ] No documentation needed

## Checklist
- [ ] Code follows project style
- [ ] Self-reviewed the code
- [ ] Added comments for complex code
- [ ] Updated README if needed
```

## Code Style

- Follow the existing code style
- Use meaningful variable and function names
- Add comments for complex logic
- Keep functions small and focused
- Write clear commit messages

## Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types:
- feat: New feature
- fix: Bug fix
- docs: Documentation changes
- style: Code style changes
- refactor: Code refactoring
- test: Adding/updating tests
- chore: Maintenance tasks

## Testing

- Write unit tests for new features
- Update existing tests when modifying code
- Ensure all tests pass before submitting PR
- Include integration tests for complex features

## Documentation

- Update documentation for new features
- Keep README up to date
- Document API changes
- Add comments for complex code

## Review Process

1. PRs require at least one review
2. Address all review comments
3. Keep the PR up to date with PulseTalk `main`
4. Squash commits if requested

## Getting Help

- Create an issue for questions
- Join our community chat
- Contact maintainers

## License

By contributing, you agree that your contributions will be licensed under the project's MIT License.
