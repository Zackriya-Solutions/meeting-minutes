# OpenRouter pull-request review

Memento reviews trusted, non-draft pull requests with Claude Sonnet 5 through a
restricted LiteLLM-to-OpenRouter route. The GitHub runner receives neither the
upstream OpenRouter key nor shell access on the gateway.

The workflow uses `pull_request_target` so it can read repository secrets, but
checks out only the trusted base revision. Pull-request code is never executed;
the diff is downloaded as untrusted data through the GitHub API. Only branches
inside this repository, opened by an owner, member, or collaborator, are
eligible.

Requests require OpenRouter zero-data-retention routing, deny provider data
collection, and disable prompt/response logging. The model must return a forced
structured review. P0/P1 findings make the check fail; P2 findings are reported
without blocking the check.

## Repository configuration

| Kind | Name | Purpose |
| --- | --- | --- |
| Actions secret | `MEMENTO_REVIEW_SSH_KEY` | Restricted SSH tunnel key |
| Actions secret | `MEMENTO_REVIEW_LITELLM_KEY` | Dedicated, budget-limited LiteLLM key |
| Actions variable | `MEMENTO_CLAUDE_REVIEW_ENABLED` | Set to `true` after a successful smoke test |

The LiteLLM key is limited to alias `memento-review-sonnet-5`, one concurrent
request, and a rolling budget. The SSH key can only forward to
`127.0.0.1:4000` on the gateway.
