# Contributing to PulseTalq

Thank you for contributing to PulseTalq. This document covers the local development workflow. All human and agent contributors follow the [multi-agent operating model](docs/MULTI_AGENT_OPERATING_MODEL.md). See [Keeping PulseTalq up to date](docs/UPSTREAM_SYNC.md) for the fork and upstream sync process.

## Development Workflow

### Branch Strategy

- `main` - Stable, releasable product
- `integration/*` - Shared candidate that receives completed task branches
- `agent/*`, `feature/*`, `fix/*`, `hotfix/*`, `docs/*`, `chore/*`, `refactor/*`, and `test/*` - One reviewable outcome per branch and worktree

Task branches start from the current target integration branch. The integration
branch starts from `main`. Only a validated integration candidate moves to
`main`.

### Getting Started

1. Clone the PulseTalq repository:
   ```bash
   git clone https://github.com/Qblaauw/PulseTalq.git
   ```
2. Add the original Meetily repository as upstream if it is not already configured:
   ```bash
   git remote add upstream https://github.com/Zackriya-Solutions/meetily.git
   ```
3. Fetch the current integration branch and create one task branch and worktree:
   ```bash
   git fetch origin --prune
   git worktree add .worktrees/your-task -b feature/TASK-ID-your-feature origin/integration/pulsetalq-next
   ```

### Development Process

1. Start from the current target `integration/*` branch
2. Create one branch and worktree for each reviewable task
3. Record the task ID, base SHA, owner, file scope, dependencies, and checks
4. Make and commit changes only in the task worktree
5. Write or update tests and documentation as needed
6. Verify the exact task-branch commit
7. Hand the completed branch to the integration coordinator

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

1. Create a PR from your task branch to the target PulseTalq `integration/*` branch
2. Link the PR to the related issue using the issue number (e.g., "Fixes #123")
3. Fill out the PR template completely
4. Ensure CI checks pass
5. Request review from at least one maintainer
6. Address any review comments
7. Once approved, the PR will be merged into the target integration branch
8. A separate validated promotion moves the integration candidate to `main`

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
3. Keep the PR up to date with its target PulseTalq `integration/*` branch
4. Squash commits if requested

## Getting Help

- Create an issue for questions
- Join our community chat
- Contact maintainers

## License

By contributing, you agree that your contributions will be licensed under the project's MIT License.
