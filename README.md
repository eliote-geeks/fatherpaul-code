<p align="center"><code>npm i -g @fatherpaul/code</code></p>
<p align="center"><strong>FatherPaul Code</strong> is a local coding assistant for the Father Paul AI ecosystem.
<p align="center">
  <img src="https://fatherpaulai.com/logo.png" alt="FatherPaul Code splash" width="80%" />
</p>
</br>
FatherPaul Code is configured for the Father Paul AI API and product surfaces:
</br><code>https://api.fatherpaulai.com/v1</code>
</br><code>https://portal.fatherpaulai.com</code>
</br><code>https://chat.fatherpaulai.com</code></p>

---

## Quickstart

### Installing and running FatherPaul Code

Install globally with your preferred package manager:

```shell
# Install using npm
npm install -g @fatherpaul/code
```

```shell
Then simply run `fatherpaul-code` to get started.

<details>
<summary>You can also go to the <a href="https://github.com/openai/codex/releases/latest">latest GitHub Release</a> and download the appropriate binary for your platform.</summary>

Each GitHub Release can contain many executables, but in practice, you likely want the FatherPaul Code binary for your platform.

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `codex-aarch64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name. Rename it to `fatherpaul-code` after extracting it.

</details>

### Using FatherPaul Code with your Father Paul AI account

Set `FATHERPAUL_API_KEY`, then run:

```shell
printenv FATHERPAUL_API_KEY | fatherpaul-code login --with-api-key
fatherpaul-code
```

The default model is `paul-code` and the default provider points to `https://api.fatherpaulai.com/v1`.

## Docs

- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Father Paul bootstrap config**](./config/fatherpaul/config.toml)

This repository is licensed under the [Apache-2.0 License](LICENSE).
