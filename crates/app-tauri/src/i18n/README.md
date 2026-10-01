# Sistema de Internacionalização e Tradução do ClearCore (i18n)

O ClearCore possui um mecanismo modular e tipado de internacionalização.
Por padrão, o idioma inicial do aplicativo é **Português (Brasil) - `pt-BR`**.

Qualquer desenvolvedor ou membro da comunidade pode contribuir adicionando novos idiomas seguindo o passo a passo abaixo.

---

## Como Adicionar um Novo Idioma

### 1. Criar o Arquivo de Idioma
Copie o modelo de `src/i18n/locales/pt-BR.ts` ou `src/i18n/locales/en-US.ts` com a sigla do idioma desejado (ex: `es-ES.ts` para Espanhol, `fr-FR.ts` para Francês):

```typescript
// src/i18n/locales/es-ES.ts
import { Translations } from '../types';

export const esES: Translations = {
  app: {
    title: 'ClearCore',
    subtitle: 'Supresión de ruido en tiempo real multiplataforma...',
    ...
  },
  ...
};
```

O TypeScript garante 100% de segurança de tipos: se faltar alguma chave, o compilador apontará exatamente qual string precisa ser traduzida.

### 2. Registrar o Idioma em `src/i18n/locales/index.ts`
Importe o seu arquivo e adicione-o ao array `availableLocales`:

```typescript
import { esES } from './es-ES';

export const availableLocales: LocaleMeta[] = [
  { code: 'pt-BR', name: 'Português (Brasil)', flag: '🇧🇷', translations: ptBR },
  { code: 'en-US', name: 'English (US)', flag: '🇺🇸', translations: enUS },
  { code: 'es-ES', name: 'Español', flag: '🇪🇸', translations: esES },
];
```

Pronto! O novo idioma aparecerá automaticamente no seletor de idiomas da interface do ClearCore e será persistido no armazenamento local do usuário.
