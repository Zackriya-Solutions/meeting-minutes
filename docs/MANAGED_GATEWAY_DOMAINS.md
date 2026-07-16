# Managed gateway domains

## Operator approval

The repository owner and gateway operator explicitly approved the migration of
Memento managed services from the legacy `gigatool.app` names to
`multitool.works` on 2026-07-16.

Approved production endpoints:

- primary: `https://gw.multitool.works`;
- failover: `https://gw2.multitool.works`;
- managed DeepSeek: `https://gw.multitool.works/deepseek/v1`.

This approval is security-sensitive because the gateways receive the
installation registration key, installation JWTs, and—after explicit user
consent—audio or transcript content for managed providers. Changes to these
hosts require a separate reviewed pull request and renewed operator approval.

## Migration verification

The following checks were performed from the development machine on
2026-07-16 without recording credentials or response bodies:

| Endpoint pair | IPv4 address | TLS | `/health` |
| --- | --- | --- | --- |
| `gw.gigatool.app` / `gw.multitool.works` | `158.160.163.167` | valid host-specific Let's Encrypt certificates | HTTP 200, identical SHA-256 |
| `gw2.gigatool.app` / `gw2.multitool.works` | `94.139.253.119` | valid host-specific Let's Encrypt certificates | HTTP 200, identical SHA-256 |

The primary and failover deployments also returned identical status and body
hashes for the tested public routes. DNS equality and response equality are
supporting migration evidence, not substitutes for the operator approval
above.

## Code safeguards

- Gateway URLs are centralized in `gateway_identity.rs`.
- Managed endpoints are restricted to exact HTTPS hosts.
- URLs cannot contain user information, query parameters, or fragments.
- The DeepSeek default reuses the centralized primary gateway constant instead
  of duplicating an independent hostname.
- Local providers remain available without sending transcripts to these
  gateways.
- Managed-provider migration remains subject to explicit user consent in the
  application.

## Rollback

If either approved host is suspected of compromise, stop distributing the
affected build, revoke the registration key and installation tokens at the
gateway, and restore endpoints only through a separately reviewed security
change. Do not silently fall back to an unapproved domain.
