# Security Policy

## Scope

This repository is a Theseus ecosystem demonstration: a multi-option prediction market with bidirectional agent-contract interactions (an agent creates markets via the contract; the contract requests an agent to resolve them). It is not deployed on a production chain and holds no production funds.

Reports that matter most:

- **A secret-shaped value lives in the current tree** (real API key, private key, signing seed). The repo has never committed an `.env*` file; if you find one, flag it.
- **The Resolver Oracle agent can be tricked into resolving a market against the on-chain truth** (signature replay, prompt injection routed through the user-provided market text, multi-resolution-cycle re-entrance).
- **The Market Creator agent can be made to mint a market the contract should have rejected** (invalid option count, expired deadline, calldata not matching the structured params the agent claims to have produced).
- **An issue in the prediction-market contract itself** that lets a participant claim more than their pool share, or that lets the resolver finalize without an attestation.

Out of scope:

- Vulnerabilities in dependencies that don't reach a live code path.
- Anything specific to Theseus runtime / SHIP language design — those belong with the [main Theseus project](https://theseus.network).

## Reporting

Email **eric@theseus.network** with "security" in the subject line.

We'll acknowledge within 72 hours and aim to confirm-or-decline within 7 days.

Please do not file a public GitHub issue for security-sensitive findings.
