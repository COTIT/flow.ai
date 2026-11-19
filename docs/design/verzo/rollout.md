# Verzo rollout

Verzo is the platform. Flow, Office, Shop, Till, and Field keep their product names. Use “Assistant” as the name of the feature. Ruaha and other partner identities retain their names and assets.

## Existing identifiers

- Keep `genesy.app` hostnames and email addresses, including staging, redirects, inbound endpoints, and fixture addresses that mirror or stand in for the existing setup. On release, update the display name in any managed production `MAIL_FROM` value to Verzo and keep its verified email address.
- Keep deployed `GENESY_*` settings, registry image paths, Compose projects and volumes, database roles and names, B2 buckets, deployment directories, service and scheduler names, and backup paths. The application accepts old `RUST_LOG` targets and emits the new Rust crate name.
- Keep browser and device state namespaces, including session, cart, till, theme, and voice keys. Keep shared cookies, headers, storefront preview messages, voice schema IDs, and assistant provider tool IDs that cross app or release boundaries. “Hey Verzo” is the current typed shortcut. Existing “Hey Genesy” input still works.
- Keep `genesy/` object keys, audit database settings such as `genesy.actor`, and cryptographic domain strings. Applied SQL migrations and SQLx query data keep their original bytes.
- Keep original evidence, historical links, provider activity names, and external repository paths where they identify an existing record or resource. Flow staging evidence stays at its original location.

Before production, smoke-test the staged release with an existing login and cart, a stored upload, and the existing assistant provider tools. Confirm that email sender display names and the three localized share cards render as Verzo.
