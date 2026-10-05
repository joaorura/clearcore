# Onda final - app (I1, I2, M5, T7)

RED (vitest, voiceProfileIpc.test.ts, antes de implementar): 8 failed | 23 passed (31). Falhas: strip/apply/errorKey inexistentes e textos antigos ('Ativo & Calibrado', 'Inscrito').
GREEN: npm run build ok; npm test -- --run: 5 arquivos, 67 passed; test:electron-state e voice-profile-ipc.selftest.cjs passaram.

Arquivos: src/VoiceProfileCard.tsx, src/__tests__/voiceProfileIpc.test.ts, src/i18n/types.ts, locales/en-US.ts, locales/pt-BR.ts.

- I1: stripServiceVoiceProfileKeys, applySetVoiceProfileResult (exportadas). As quatro chamadas set_voice_profile enviam o status sem chaves de servico e aplicam o resultado (ausente/offline => chaves de servico limpas). (c) Assinante onVoiceProfileUpdate adicionado (useEffect, mesmo merge).
- I2: statusActive => "Aplicado no serviço (desenvolvimento)" / "Applied in service (development)"; nova chave appliedInServiceNote exibida junto ao estado active.
- M5: voiceProfileErrorKey (codigos + mensagens genericas do servico; texto livre => errorUnknown); 12 chaves i18n errorXxx nos dois locales.
- T7: "Cadastrado, ativação não confirmada".
Nao feito: main.cjs/Rust intocados. Sem teste de componente (sem DOM/RTL no projeto); logica coberta por funcoes puras.
