# Offline Dependency Gate: UI Package Resolution

## Status: `BLOCKED_OFFLINE_DEPENDENCY`

- **Timestamp**: 2026-09-30T22:54:38Z
- **Target**: `crates/app-tauri`
- **Mandate**: `docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md` (Task 14, Line 711)

---

## Command Execution

```bash
cd crates/app-tauri && npm install --package-lock-only --offline --ignore-scripts
```

### Observed Output & Error Log

```text
npm error code ENOTCACHED
npm error request to https://registry.npmjs.org/@tauri-apps%2fapi failed: cache mode is 'only-if-cached' but no cached response is available.
npm error A complete log of this run can be found in: ~/.npm/_logs/2026-09-30T22_54_38_802Z-debug-0.log
```

---

## Policy Assessment & Compliance

1. **Hermetic Offline Boundary**:
   Per project policy and Task 14 plan specifications, unauthorized internet access during offline qualification is prohibited. No alternative ad-hoc download or network bypass was attempted.

2. **Backend Rust Verification**:
   The Rust backend component (`realtime-noise-app-tauri`) and the diagnostics library (`realtime-noise-diagnostics`) compile and pass all tests hermetically inside the offline Docker container (`--network none`, `--locked`, `--offline`).

3. **Required Pre-seeding**:
   To generate `package-lock.json` and build the desktop bundle in offline environments, the local npm cache (`~/.npm`) must be pre-populated with:
   - `@tauri-apps/api@2.0.0`
   - `@tauri-apps/plugin-shell@2.0.0`
   - `react@18.3.1`
   - `react-dom@18.3.1`
   - `@tauri-apps/cli@2.0.0`
   - `@types/react@18.3.12`
   - `@types/react-dom@18.3.1`
   - `@vitejs/plugin-react@4.3.4`
   - `typescript@5.6.3`
   - `vite@5.4.10`
   - `vitest@2.1.4`

4. **Lifecycle Segregation Confirmed**:
   The Tauri frontend is purely a monitoring and control client. The core DSP engine and service daemon (`realtime-noise-service`) do not depend on npm or Tauri and remain fully operational and testable offline.
