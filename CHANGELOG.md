# Changelog

All notable changes to FluxGuard will be documented here.

## [0.2.2](https://github.com/suiflex/FluxGuard/compare/v0.2.1...v0.2.2) (2026-09-16)


### Bug Fixes

* **ci:** isolate test scratch directories and update rustls ([#21](https://github.com/suiflex/FluxGuard/issues/21)) ([b60c0b0](https://github.com/suiflex/FluxGuard/commit/b60c0b0cab5d645ba9e3fef53c12e894aa5343c4))

## [0.2.1](https://github.com/suiflex/FluxGuard/compare/v0.2.0...v0.2.1) (2026-09-16)


### Code Refactoring

* **cli:** register install clients through kurir ([#18](https://github.com/suiflex/FluxGuard/issues/18)) ([7c1595a](https://github.com/suiflex/FluxGuard/commit/7c1595aff8bdbe703da4e7b821c9bdfb08f590e7))

## [0.2.0](https://github.com/suiflex/FluxGuard/compare/v0.1.3...v0.2.0) (2026-09-12)


### Features

* **adapters:** add anthropic api provider adapter ([763607d](https://github.com/suiflex/FluxGuard/commit/763607d438f5c1a3deca35da1cdc7fc619395192))
* **adapters:** add claude code client telemetry adapter ([6eefce6](https://github.com/suiflex/FluxGuard/commit/6eefce68cc928514eb51436f4037a983abe8655c))
* **adapters:** add cursor client telemetry adapter ([76d30c3](https://github.com/suiflex/FluxGuard/commit/76d30c31bef0fe756a6c3e4c46f7f0ba3d715f14))
* **adapters:** add github copilot client adapter ([b64e5b1](https://github.com/suiflex/FluxGuard/commit/b64e5b1fa1834aee8ec523e661fff4d7adc77a54))
* **adapters:** add google antigravity client adapter ([bb9146e](https://github.com/suiflex/FluxGuard/commit/bb9146eaabf6676e44d67573910737807eb78e83))
* **adapters:** add openai api provider adapter ([b8e2f20](https://github.com/suiflex/FluxGuard/commit/b8e2f20c25de38f296555787d13cb27986d8f3eb))
* **adapters:** add xai api provider adapter ([ca2467e](https://github.com/suiflex/FluxGuard/commit/ca2467e7bba1047ad7aab5e85a18979eeac17145))
* **adapters:** add zai glm coding plan provider adapter ([f9e14a4](https://github.com/suiflex/FluxGuard/commit/f9e14a41eeb585b50110292557b95262489e465f))
* **adapters:** expand client and provider matrix ([be43479](https://github.com/suiflex/FluxGuard/commit/be43479ac9335a926aa3c166d61d600b15dcd6da))
* **cli:** add update command, config editor, and install menu ([#16](https://github.com/suiflex/FluxGuard/issues/16)) ([ad935e9](https://github.com/suiflex/FluxGuard/commit/ad935e93d79c6c1069f53061f8da815710468a67))
* **cli:** wire matrix adapters into config and doctor command ([b8b7f0c](https://github.com/suiflex/FluxGuard/commit/b8b7f0c9889b9fbe14e6d1c27faacaf50d1f88e1))
* **runtime:** add publish_once run strategy for one-shot sources ([d9c975d](https://github.com/suiflex/FluxGuard/commit/d9c975d7b825510c721a78df0358bc3767db3b09))


### Bug Fixes

* **adapters:** make anthropic adapter report unsupported instead of empty data ([21780ab](https://github.com/suiflex/FluxGuard/commit/21780abd5d35cb87d89f723c6f6a679fd6758c9f))
* **adapters:** make antigravity adapter report unsupported instead of empty data ([012995d](https://github.com/suiflex/FluxGuard/commit/012995def88888c952d1a92638ca7d57a3e0dc15))
* **adapters:** make claude_code adapter report unsupported instead of empty data ([9040cc1](https://github.com/suiflex/FluxGuard/commit/9040cc137c28ba18c8fec72d5b97cbe5ad051bf3))
* **adapters:** make cursor adapter report unsupported instead of empty data ([7137c1a](https://github.com/suiflex/FluxGuard/commit/7137c1ae17e1c7180ec81e9389e0ef555cda383c))
* **adapters:** make openai adapter report unsupported instead of empty data ([ccd6745](https://github.com/suiflex/FluxGuard/commit/ccd67451594d320a67e0c94e66eafba99a22c20e))
* **adapters:** make xai adapter report unsupported instead of empty data ([012b061](https://github.com/suiflex/FluxGuard/commit/012b0614886ccd23897d5c6228ec0e01d91b507f))
* **adapters:** make zai adapter report unsupported instead of empty data ([e69bb1b](https://github.com/suiflex/FluxGuard/commit/e69bb1bde1af9314f26d41e58ac2c38358c85873))
* **adapters:** read copilot quota via headless cli account.getQuota ([b70291a](https://github.com/suiflex/FluxGuard/commit/b70291aa03489538b5272bb551fdee73de10ac4c))
* **cli:** flag detection-only sources in doctor output ([07509f5](https://github.com/suiflex/FluxGuard/commit/07509f500714ac8f375553e6eeabbceaf89e7f5d))
* **runtime:** stop the default run loop when a source is unsupported ([7d43ed3](https://github.com/suiflex/FluxGuard/commit/7d43ed3f82413b916719538340c4d19684953e12))

## [0.1.3](https://github.com/suiflex/FluxGuard/compare/v0.1.2...v0.1.3) (2026-09-12)


### Bug Fixes

* **ci:** remove --locked and add retry loop for crates publish ([e9db53d](https://github.com/suiflex/FluxGuard/commit/e9db53dbc0f97337c104ead2559f49130c48b94f))
* **ci:** remove --locked and add retry loop for crates publish ([38ce8e8](https://github.com/suiflex/FluxGuard/commit/38ce8e8ae7b02d03050b5da25da8f7fd04dfe0cc))

## [0.1.2](https://github.com/suiflex/FluxGuard/compare/v0.1.1...v0.1.2) (2026-09-12)


### Bug Fixes

* **ci:** align release publish workflow with safehell standard ([b58bcec](https://github.com/suiflex/FluxGuard/commit/b58bcec8301128916ef07206051782ef390f56c1))
* **ci:** align release-build workflow and formula template with SafeHell ([d278de3](https://github.com/suiflex/FluxGuard/commit/d278de37a57d6cf1a3590d7f32adfcf610d1e1a7))

## [0.1.1](https://github.com/suiflex/FluxGuard/compare/v0.1.0...v0.1.1) (2026-09-12)


### Bug Fixes

* **ci:** update actions/labeler to valid v7.0.0 commit sha ([8e45c10](https://github.com/suiflex/FluxGuard/commit/8e45c10e5e3830d89f3275d9993ca4b4e7e56144))
* **ci:** use valid commit sha for release-please-action ([a71b8dd](https://github.com/suiflex/FluxGuard/commit/a71b8ddd692d35da427563270cf79fe7d6310464))
* **ci:** use valid commit sha for release-please-action ([0529e11](https://github.com/suiflex/FluxGuard/commit/0529e1135e5509d1aeb7f5e378e5e7aa4064a81c))

## 0.1.0 (2026-09-12)


### Features

* **brand:** add project logos and mark assets ([afa8a55](https://github.com/suiflex/FluxGuard/commit/afa8a55e68ca28996e822632eaff217299387c42))
* **install:** add posix and windows installer scripts ([40e105e](https://github.com/suiflex/FluxGuard/commit/40e105edf74f53e37b131be3b7354ed72855e510))


### Bug Fixes

* **adapters:** scope test imports to unix-only test for windows clippy ([5ce63ec](https://github.com/suiflex/FluxGuard/commit/5ce63ec1b2d92151bf911bea464c25fc3e7ffd69))
* **ci:** align workflow triggers and release target to develop ([db02bea](https://github.com/suiflex/FluxGuard/commit/db02beaf471abb37b328a99ff7da5604d26a5bb6))
* **ci:** align workflow triggers and targets to develop branch ([d1f2f81](https://github.com/suiflex/FluxGuard/commit/d1f2f81a8b5cbccda7dbeab9a76a30f0bef6c6a7))

## [Unreleased]

- Bootstrap the Rust workspace and crate boundaries.
