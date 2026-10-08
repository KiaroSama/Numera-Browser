# Numera Browser

A browser-profile manager that preserves coherent identities and owns each browser session from launch through verified closure.

## Language

**Profile**:
A persisted browser identity together with its chosen engine, proxy and editable settings.
_Avoid_: Account, browser instance

**Fingerprint**:
The coherent observable browser identity associated with a profile, including its seeded generated characteristics.
_Avoid_: Random settings

**Session claim**:
The durable record that a particular owned browser tree still holds a profile, even if its launcher has died.
_Avoid_: Running flag, process marker

**Session lease**:
The exclusive right to operate one profile while its session is being opened or held.
_Avoid_: Session claim

**Vault generation**:
An authenticated set of credential envelopes and the descriptor needed to open them safely.
_Avoid_: Password cache

**Browser engine**:
One supported backend that applies a profile identity and supplies a real browser session.
_Avoid_: Application engine

**Application engine**:
The shared profile, storage, automation, authorization and lifecycle behavior behind the CLI and dashboard.
_Avoid_: Browser engine

**Browser adapter**:
The narrow boundary to a browser backend's SDK; it is not a substitute for the application engine.
_Avoid_: Python engine wrapper
