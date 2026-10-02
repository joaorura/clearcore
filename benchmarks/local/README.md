# Resultados locais do benchmark de CPU

Esta pasta guarda relatórios de `scripts/run-cpu-baseline-local.sh`. Os arquivos
`cpu-baseline-*.json|md` são específicos da máquina e ficam fora do git (ver `.gitignore`).
O baseline commitado continua sendo `benchmarks/cpu-baseline.json`; ele nunca é sobrescrito
por este fluxo.

## Como rodar

    scripts/run-cpu-baseline-local.sh

O script roda o benchmark oficial numa cópia descartável de `HEAD` (o binário grava
`benchmarks/cpu-baseline.*` relativo ao diretório atual).

## Status possíveis

- `BLOCKED_UNSUPPORTED_CPU_PROFILE`: o host não apresentou evidência qualificada. Nenhuma latência
  é medida e nenhum número deve ser inferido deste relatório. Evidência qualificada exige uma
  máquina de referência (x86_64, `i5-10210U`, 4 núcleos físicos, sem virtualização), um registro
  de observação de 300 s gerado por coletor externo e os três argumentos
  `--host-evidence`, `--host-evidence-provenance` e `--run-id` (caminhos absolutos).
- `BLOCKED_NO_APPROVED_ASSET`, `BLOCKED_PENDING_GOLDEN`: o asset aprovado ou o golden não foi
  verificado; o campo `measurements.reason` traz o motivo.
- `M1_APPROVED`: único status com latência medida.

O relatório mede só a latência do worker de inferência, não a latência fim a fim do produto.
