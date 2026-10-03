# Amostras de voz e ruído para testes

Áudio **nunca** vai para o git. O script `scripts/fetch-voice-samples.sh` baixa um conjunto pequeno
(cerca de 15 MB) para `fixtures/voice-samples/` (pasta no `.gitignore`), confere o SHA-256 contra
`scripts/voice-samples.sha256` e grava `fixtures/voice-samples/ATTRIBUTION.txt`. Testes que usam áudio
**pulam** quando os arquivos não existem, então o CI offline continua verde.

## Como usar

```bash
scripts/fetch-voice-samples.sh                  # conjunto padrão (VCTK + 3 ruídos CC0)
scripts/fetch-voice-samples.sh --with-tagarela  # inclui o shard de teste do TAGARELA (opcional)
scripts/fetch-voice-samples.sh --from-dir DIR   # usa arquivos já baixados, sem rede
scripts/fetch-voice-samples.sh --verify-only    # só confere os hashes dos arquivos presentes
```

Requisitos: `curl`, `sha256sum` (ou `shasum`), `ffmpeg` e `ffprobe`, e `python3` com o módulo `venv`
(no Debian/Ubuntu, o pacote `python3-venv`). O script cria `fixtures/voice-samples/.venv` e instala
`remotezip==0.12.6` ali; nada é instalado no sistema. O zip completo do VCTK tem 11,7 GB e **não** é
baixado: o `remotezip` pede por HTTP Range só os seis arquivos necessários.

Para mudar o destino: `FIXTURES_DIR=/outro/caminho scripts/fetch-voice-samples.sh`.

## O que é baixado

| Arquivo | Origem | Licença |
|---|---|---|
| `speech/p225_{003,008,011,022}_mic1.flac`, `speech/p226_{008,016}_mic1.flac` | VCTK Corpus 0.92 (`wav48_silence_trimmed`), 48 kHz | CC BY 4.0 |
| `noise/street_traffic_rain_cc0_10s.wav` | Wikimedia Commons, "Urban Street on a Rainy Afternoon" (5 s a 15 s) | CC0 |
| `noise/forest_rain_cc0_10s.wav` | Wikimedia Commons, "Light Rain Distant Thunder July 5th 2016" (0 s a 10 s) | CC0 |
| `noise/ac_fan_cc0_10s.wav` | Wikimedia Commons, "Resident air-conditioned out door unit" (5 s a 15 s) | CC0 |
| `tagarela/test-00000-of-00001.parquet` (só com `--with-tagarela`) | Hugging Face `freds0/TAGARELA`, shard de teste, 78 277 756 bytes | CC BY-NC-SA 4.0 |

Os clipes de ruído são recortes de 10 s, mono, PCM s16le, 48 kHz, feitos pelo `ffmpeg`. A `p225_011`
(6,8 s) serve de fala de enrollment. As falas do VCTK são em inglês: não há fala em pt-BR no
conjunto padrão (ver o TAGARELA abaixo e a seção "Outros conjuntos em pt-BR").

## Atribuição (obrigatória)

VCTK: *CSTR VCTK Corpus v0.92*, University of Edinburgh, The Centre for Speech Technology Research
(CSTR); Yamagishi et al. Licença CC BY 4.0: https://datashare.ed.ac.uk/handle/10283/3443

Os ruídos são CC0 (sem exigência de atribuição; as fontes estão em `ATTRIBUTION.txt`).

## TAGARELA: só dado de teste local

O TAGARELA é **CC BY-NC-SA 4.0**, derivado dos "Cem Mil Podcasts" (direitos dos podcasters não
esclarecidos), a 16 kHz, já passado por vocoder-denoiser e sem speaker id garantido. Regras:

- só com a flag explícita `--with-tagarela`, nunca no conjunto padrão;
- **nunca commitar** o arquivo nem qualquer áudio derivado dele;
- o script não extrai áudio: o `.parquet` fica como baixado;
- **fora do treino de pesos que serão distribuídos**, até o dono do projeto registrar decisão
  explícita (o NC-SA pode exigir que os pesos herdem CC BY-NC-SA, em conflito com a licença dos pesos próprios; a partir de 2026-10-03 os pesos próprios são CC BY-NC 4.0, mas isso não dispensa a decisão do dono).

## Derivados e versão do ffmpeg

Os hashes dos `noise/*.wav` valem para `ffmpeg 8.1.3`. Com outra versão o script emite `WARNING` e
mantém o arquivo; os originais (`speech/*`, `tagarela/*`) divergentes abortam.

## Convenção para testes que usam áudio

Procure o arquivo e **pule** o teste se ele faltar (mensagem em `stderr`, `return` sem falhar):

```rust
fn voice_sample(relative: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/voice-samples")
        .join(relative);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skip: {relative} ausente; rode scripts/fetch-voice-samples.sh");
        None
    }
}
```

## Outros conjuntos em pt-BR

Conferido em 2026-10-02, **sem baixar nenhum conjunto** (liberados para uso, inclusive treino, pelo
usuário em 2026-10-01; a decisão sobre pesos distribuídos continua com o dono do projeto).

| Conjunto | Licença | Taxa de amostragem | Acesso | Status da conferência |
|---|---|---|---|---|
| **MLS Portuguese** (Multilingual LibriSpeech, OpenSLR SLR94) | **CC BY 4.0** (https://www.openslr.org/94/) | **16 kHz**, mono, FLAC (ou Opus) | Sem login: https://dl.fbaipublicfiles.com/mls/mls_portuguese.tar.gz (9,3 GB) e `mls_portuguese_opus.tar.gz` (2,5 GB) | Confirmado na fonte: licença na página do OpenSLR; 16 kHz no artigo (arXiv 2012.03411, seção 3.1) e em um FLAC real lido por HTTP Range (`ffprobe`: `flac,16000,1`) |
| **Common Voice pt** (Mozilla Data Collective, "Common Voice Scripted Speech 23.0 - Portuguese") | CC0-1.0 segundo indício indireto | MP3; 16 kHz segundo relato secundário (originais a 48 kHz) | Exige conta e aceite de termos em https://mozilladatacollective.com (ficha: `/datasets/cmflnn483xz7xpuiogors5llv`) | **Não confirmado na fonte oficial**: a ficha é renderizada no cliente e a API devolveu 404 |

Atribuição do MLS (CC BY 4.0): Pratap, Xu, Sriram, Synnaeve, Collobert. *MLS: A Large-Scale
Multilingual Dataset for Speech Research*, arXiv:2012.03411. O áudio vem de audiolivros do LibriVox.

Consequência técnica: o pipeline do Clearcore é de 48 kHz fullband. Os dois conjuntos chegam a
16 kHz, então servem para avaliar fala e enrollment em pt-BR, não para validar banda acima de 8 kHz
(de-esser, EQ de brilho). Nenhum dos dois é baixado por `fetch-voice-samples.sh`.
