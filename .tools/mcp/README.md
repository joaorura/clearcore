# Ferramentas MCP / AI Code Intelligence

Este diretório contém a configuração e metadados dos servidores MCP integrados para o projeto e para o Antigravity (`agy`):

1. **[Serena](https://github.com/oraios/serena)** (`serena-agent`):
   - Executável: `serena`
   - Comando MCP: `serena start-mcp-server --context=antigravity --project-from-cwd`

2. **[CodeGraph](https://github.com/colbymchenry/codegraph)** (`@colbymchenry/codegraph`):
   - Executável: `codegraph`
   - Comando MCP: `codegraph serve --mcp`
   - Base de dados e índice: `.codegraph`

3. **[Enquire](https://github.com/oomkapwn/enquire-mcp)** (`@oomkapwn/enquire-mcp`):
   - Executável: `enquire-mcp`
   - Comando MCP: `enquire-mcp serve --vault <caminho>`
   - Indexação híbrida para documentos e anotações Markdown do projeto.

---

### Arquivos de Configuração

- [`mcp-config.json`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/.tools/mcp/mcp-config.json): Arquivo de configuração de referência dentro do projeto.
- [`~/.gemini/config/mcp_config.json`](file:///home/joaorura/.gemini/config/mcp_config.json): Arquivo de configuração ativo do Antigravity CLI (`agy`).
