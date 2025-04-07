# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---
## [0.3.0](https://github.com/BoltzExchange/hold/compare/v0.2.2..v0.3.0) - 2025-04-01

### Bug Fixes

- correct error for too little CLTV - ([489d659](https://github.com/BoltzExchange/hold/commit/489d6596bd9b454c18ddc365fb7d3cab5d721d9e))

### Features

- BOLT12 hold invoices - ([925716a](https://github.com/BoltzExchange/hold/commit/925716ae82b91468c8ad95f7ef8b2d08b41e4f27))
- onion message subscription - ([5975c4f](https://github.com/BoltzExchange/hold/commit/5975c4fb0eb7ca919b023c944b11f23197095df4))
- explicit min CLTV - ([7e786f3](https://github.com/BoltzExchange/hold/commit/7e786f33c8bc8bc3fa496096272141e99d610e49))
- onion message resolver - ([4894341](https://github.com/BoltzExchange/hold/commit/4894341e7381b0a6322b14353afb9b3b5a3f125d))

### Miscellaneous Chores

- bump CHANGELOG to v0.2.2 - ([7c27d49](https://github.com/BoltzExchange/hold/commit/7c27d49e11c35400d9281ce15a2b9900944f4729))
- rename bolt11 to invoice - ([982f840](https://github.com/BoltzExchange/hold/commit/982f8402244fc294ee045a9a6e032cff9080ad84))
- clippy fixes - ([390f5bc](https://github.com/BoltzExchange/hold/commit/390f5bc4182c48b313fd057a56c72fbcbf39bdc7))
- fix CI on updated Ubuntu image - ([3894624](https://github.com/BoltzExchange/hold/commit/38946242b716ab0cc90b5aced464fcbaf5748710))
- bump dependencies - ([2b33ad4](https://github.com/BoltzExchange/hold/commit/2b33ad41f3ca6ca62ade6e9b46b2f7e47adf059e))
- move startup log to be printed - ([d85b771](https://github.com/BoltzExchange/hold/commit/d85b7716dd927a2b3b872c92cf701ec94368e3d3))
- update regtest - ([4aad423](https://github.com/BoltzExchange/hold/commit/4aad423e9970dd8886bb2b292f0e69b2991f4dd0))

### Refactoring

- rename blinding to first_path_key - ([3314e6d](https://github.com/BoltzExchange/hold/commit/3314e6d41a3a1e520ebb4223afc7df96a19614cb))
- continue onion messages when there are no subscribers - ([2a37ce5](https://github.com/BoltzExchange/hold/commit/2a37ce50ff7450e571aaff56c6f3cdc7e442a2eb))

---
## [0.2.2](https://github.com/BoltzExchange/hold/compare/v0.2.1..v0.2.2) - 2025-03-08

### Bug Fixes

- set SQLite busy_timeout - ([6c6fdd9](https://github.com/BoltzExchange/hold/commit/6c6fdd9cdd268cae4a95226c0bd227b731a362cd))
- double event race condition - ([9048b7f](https://github.com/BoltzExchange/hold/commit/9048b7f5579ef15e530d1b8309765c5adad93ade))

### Miscellaneous Chores

- **(deps)** bump ring from 0.17.8 to 0.17.13 (#14) - ([07f69e7](https://github.com/BoltzExchange/hold/commit/07f69e75b68ac2b670ce6ebcdf064cf3cf3271a6))
- bump regtest - ([4cccfdd](https://github.com/BoltzExchange/hold/commit/4cccfddb2bae083e05b68867cfd3867ebcbcaf34))
- stricter clippy rules - ([0c90f8e](https://github.com/BoltzExchange/hold/commit/0c90f8e206776ce2ccc9b58594285ae990aa9a23))
- bump version v0.2.2 - ([0e9126a](https://github.com/BoltzExchange/hold/commit/0e9126a70a09ef009e82071df00d6c52d7f979bb))

---
## [0.2.1](https://github.com/BoltzExchange/hold/compare/v0.2.0..v0.2.1) - 2024-12-30

### Features

- add settled_at to invoices (#11) - ([43d7c03](https://github.com/BoltzExchange/hold/commit/43d7c03db135f601a7778eeb76ac08321a40ede2))

### Miscellaneous Chores

- minor dependency updates (#9) - ([6004385](https://github.com/BoltzExchange/hold/commit/6004385bf0b7382aab7824cbadd251ba17e6a962))
- bump minor dependencies (#10) - ([b6d362d](https://github.com/BoltzExchange/hold/commit/b6d362d92c65dc0156b354ee834c6a5ff11c9da6))
- add CHANGELOG.md - ([945bca9](https://github.com/BoltzExchange/hold/commit/945bca9779cff2c91631a0e9b4246545069206cf))

---
## [0.2.0](https://github.com/BoltzExchange/hold/compare/v0.1.2..v0.2.0) - 2024-11-07

### Features

- send last update in TrackAll stream (#8) - ([e0d7658](https://github.com/BoltzExchange/hold/commit/e0d76583fe4e36c9ffdb4fdd5a2b807a8fe8cd6f))
- cleaning of cancelled invoices - ([e227c83](https://github.com/BoltzExchange/hold/commit/e227c83a3a3ef9d80995b73443ee272161d3e85f))

### Miscellaneous Chores

- update dependencies - ([98a22ce](https://github.com/BoltzExchange/hold/commit/98a22ce01690570e0cf62b4cd8e7a3bde19a453b))
- bump version to 0.2.0 - ([f0409d0](https://github.com/BoltzExchange/hold/commit/f0409d013da25516867bcc0576d22708c466b2f0))

---
## [0.1.2](https://github.com/BoltzExchange/hold/compare/v0.1.1..v0.1.2) - 2024-10-18

### Miscellaneous Chores

- allow calling settle and cancel multiple times - ([8d3d88c](https://github.com/BoltzExchange/hold/commit/8d3d88cea3a41246b691bde9415034a53c84bdb2))
- switch from poetry to uv - ([f6d4c84](https://github.com/BoltzExchange/hold/commit/f6d4c840ffab643e6e1165fab5f6a1d9c5fbea35))
- bump version to v0.1.2 - ([81a3189](https://github.com/BoltzExchange/hold/commit/81a3189c2b85bf45b65dbbfe3cfe629cbacc16d8))

---
## [0.1.1](https://github.com/BoltzExchange/hold/compare/v0.1.0..v0.1.1) - 2024-10-08

### Miscellaneous Chores

- **(deps)** bump diesel from 2.2.2 to 2.2.3 (#3) - ([576f9cb](https://github.com/BoltzExchange/hold/commit/576f9cb769859b01302c21a92400f62fdd4daa0c))
- add release build script - ([272794e](https://github.com/BoltzExchange/hold/commit/272794e6954ec18121dda4da78cef918e93ff2b2))
- bump regtest version - ([ea3f498](https://github.com/BoltzExchange/hold/commit/ea3f4985756ff667206ad3a67e86726b358f1009))
- update regtest - ([2589108](https://github.com/BoltzExchange/hold/commit/25891089081c967cd897cb6f084c3a0b715d3c4e))
- update dependencies - ([e036750](https://github.com/BoltzExchange/hold/commit/e03675092a50a183a214632805a2c1e6455c58f2))
- bump version to v0.1.1 - ([d8e8578](https://github.com/BoltzExchange/hold/commit/d8e8578a6fe82688c5ccef14ba12b1ca7280088e))

### Tests

- add smoke tests with pyln-testing (#4) - ([d9eb28e](https://github.com/BoltzExchange/hold/commit/d9eb28ea6238a25dc297b42446721ce8ad2f672f))

---
## [0.1.0] - 2024-08-22

### Bug Fixes

- state updates with no gRPC listeners - ([36c641a](https://github.com/BoltzExchange/hold/commit/36c641a42c5b1b8a798b643a12da5ded464d5b5a))

### Features

- mvp implementation - ([3c0c8d9](https://github.com/BoltzExchange/hold/commit/3c0c8d90abf014455e478273d01fd8b13a4e75d4))
- missing gRPC methods - ([12837dc](https://github.com/BoltzExchange/hold/commit/12837dc267f592affb686d4326940f97e6f8dc62))
- handle already known HTLCs - ([f701bd3](https://github.com/BoltzExchange/hold/commit/f701bd3853d399a4ee95beb30bdf5e616b3178a6))
- MPP timeouts - ([0038db8](https://github.com/BoltzExchange/hold/commit/0038db826b3ae2e16f33029a666fc434f656cbac))
- PostgreSQL support - ([a42582f](https://github.com/BoltzExchange/hold/commit/a42582f6385b1d3ee9d18d5fa4a0cf0ec19309d8))
- forbid invalid invoice state transitions - ([32f4c4d](https://github.com/BoltzExchange/hold/commit/32f4c4dedb9a4f0c338fb0a2d2e825147f205cdf))

### Miscellaneous Chores

- add CI checks (#1) - ([c402889](https://github.com/BoltzExchange/hold/commit/c4028892e5e0e24ad3800d91858f74559a1a8f87))
- cleanup CI workflow - ([3ff0ef1](https://github.com/BoltzExchange/hold/commit/3ff0ef1be54455b6c640a29a56c60395af2a9dbd))
- include version in startup message - ([f0306db](https://github.com/BoltzExchange/hold/commit/f0306db119a1be4491d6ddc67dc97fe0cecbc559))
- add README - ([50c69ba](https://github.com/BoltzExchange/hold/commit/50c69bada94c49886e22b2e96927f4f4fb367e49))

### Refactoring

- add basic usage description for RPC commands - ([03c84d5](https://github.com/BoltzExchange/hold/commit/03c84d5f8d1b580b3576d03a136dda315416daf1))

### Tests

- add unit tests - ([b333eef](https://github.com/BoltzExchange/hold/commit/b333eefee1225c92c7e9409055795f5b390503f2))
- RPC command integration tests - ([8368516](https://github.com/BoltzExchange/hold/commit/8368516bdaab31564b28c705ed3695d6cc42d358))
- gRPC integration tests - ([67b7437](https://github.com/BoltzExchange/hold/commit/67b7437ec0b79ff9915370b168128861de4b3fd9))
- HTLC handling - ([1dff07b](https://github.com/BoltzExchange/hold/commit/1dff07b65c6db75803f8ecff63a4f700be96f728))

<!-- generated by git-cliff -->
