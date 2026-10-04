# Etta — handoff

Estado em **2026-10-04**. O marco "estabilidade no Halley + Finder, menu,
cápsula do Haven" está no commit que acompanha esta versão do arquivo.
Escrito para retomar numa sessão nova sem reler a conversa anterior.

---

## 1. O que é o projeto

**Etta** é um shell de desktop para o compositor Wayland **Halley**
(<https://github.com/saltnpepper97/halley>, 0.7.0, API 1). Ela nasceu da
**Marea** (`k4ditano/marea-plm`, BSD-3) a partir do fork `ely-id/marea-plm`,
branch `halley-compat`. O fork tem o adaptador Halley e foi importado sem
histórico. A atribuição da licença está em `LICENSE` e `NOTICE.md`.

- **O nome:** o cometa 1P/Halley deixa a chuva de meteoros Eta Aquáridas; daí
  Etta.
- **O runtime:** é o **pleamar 0.2.8** instalado. O pleamar **não é forkado**:
  fonte em `~/Work/pleamar`, origin `k4ditano/pleamar`, commit `4163c91`;
  binário em `~/.local/bin/pleamar`; backup do 0.2.6 em
  `~/.local/state/etta-pleamar-0.2.6.bak`.
- **Idioma:** o produto (textos, comentários, commits) é **só em inglês**. A
  conversa com o usuário é em português.

### Regras que não mudam

1. **O pet fica intacto:** corpo, rosto, acessórios, movimentos, animações e
   reações. Só pode mudar **onde ela interage com os painéis**, mais os ajustes
   aprovados:
   - tamanho maior (56 px padrão, opções 46/52/56/64);
   - glifos de OSD nos olhos, grossos o bastante para manter a cara dela;
   - o morph do cadeado na tela de bloqueio;
   - as peças novas do guarda-roupa.
2. **Prioridades:** arquitetura, depois funcionalidade, depois UX/UI.
3. **Ações externas ou irreversíveis precisam de confirmação:** push, publicar,
   mudar a config do usuário. O usuário aprova cada coisa.
4. **Commits:** um por marco, terminando com
   `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Identidade git
   local: "Ely".

### Etapas

- Etapa 1 (entendimento): ✓
- Etapa 1.5 (estudo): ✓
- Etapa 2 (protótipos interativos): ✓, aprovada. Canvas com 27 artboards em
  <https://claude.ai/artifact/BtBjMxWBmidS49AJUyvfMv>, arquivos
  `project/*.dc.html`. É a **referência visual de tudo**.
- **Etapa 3 (implementação): em andamento.**
  - Fase 1: UX/UI.
  - Fase 2: integração correta com o Halley.
  - As duas fases estão andando juntas, porque cada marco já é testado ao vivo
    no Halley.

---

## 2. Onde as coisas estão

| Caminho | O que é |
| --- | --- |
| `etta.plm` | A cena (≈4 900 linhas): o pet, as superfícies e as regras. Importa `ui/*.plm` e `common/*.plm` |
| `etta.luau` | A lógica: serviços, persistência e processos externos |
| `ui/tokens.plm` | Tokens de design (`tk.*`: cores, escala de tipo, raios). Cada token documenta seu uso |
| `ui/kit.plm` | Kit: `Btn`, `Ib`, `Toggle`, `Slider`, `Seg` (até 5 itens), `Chip`, `Tag`, `Kbd`, `Row`, `Bubble`, mais as figuras `Kit.i_*` (71 ícones SVG em `ui/icons/`) |
| `ui/panels.plm` | Todas as páginas do control center como componentes (veja §4) |
| `ui/showcase.plm` | Cena de vitrine do kit |
| `common/accessories.plm` | Física das peças: `Perched`, `Pinned`, `Tagalong`. **Não mexer** (é do pet) |
| `wardrobe/*.svg` | As 20 peças |
| `wallpapers/*.frag` | 6 field shaders próprios, mais `space.png` (foto do shader do Halley; veja `NOTICE.md`) |
| `halley/` | Adaptador `etta-halley` (Rust): `outputs` com transform e scale, `window …`. Testes: `cargo test --release --offline` |
| `tools/shader-thumb/` | `etta-shader-thumb`: GLSL ES do Halley → GLSL 450 → glslangValidator → SPIR-V → wgpu → PNG. Compila offline |
| `tools/reservas` | Leitor (node) das cotas de Claude Code, Codex, Copilot (via `gh api /copilot_internal/user`) e Grok (sem API: o card diz isso) |
| `deriva/` | Biblioteca Deriva (`deriva-worker`, já compilado) |
| `etta` | Launcher: `start`, `stop`, `status`, `report` e `<evento>`. Coloca `halley/`, `tools/shader-thumb/` e `deriva/` no PATH, exporta `ETTA_DIR` e roda `--stall 0` |
| `docs/contract/` | Contrato herdado da Marea (animações e medidas) |

**Dados em tempo de execução:**

- `~/.local/share/pleamar/etta/`: `settings.json`, `history.json`,
  `halley-wallpaper.rune`.
- Log: `~/.local/state/etta/etta.log`. O launcher guarda **um** log anterior,
  então uma segunda reinicialização apaga o log de um crash.
- Cache: `~/.cache/etta/` (miniaturas dos shaders e capas de mídia).
- Shaders do usuário: `~/.config/etta/shaders/`.

---

## 3. Como rodar e testar (na máquina do usuário)

```sh
etta start --screen HDMI-A-1        # ou só `etta start`; o link está em ~/.local/bin/etta
etta status
pleamar --check etta.plm            # compila a cena com tudo o que importa
pleamar --say etta "fact open true" # faz algo ao vivo: fact X V · get X · emit EVENTO [payload]
grim -o HDMI-A-1 /tmp/s.png && magick /tmp/s.png -rotate 180 /tmp/v.png   # o HDMI está físico a 180°
```

- **Hot reload:** a cena e a lógica recarregam ao salvar.
- **O que exige reiniciar a instância:**
  - superfícies novas;
  - variáveis de ambiente.
- **Permissões** (`permissions { run: …; services: … }`) recarregam ao
  salvar no pleamar 0.2.8: o log diz "permissions now: …". Mesmo assim, todo
  binário novo chamado com `run()` precisa entrar na lista.
- **Para reiniciar sem matar o próprio shell:**
  `kill $(pgrep -f "^/home/ely/.local/bin/pleamar --scene etta.plm")`, depois
  `etta start`. **Não use** `pkill -f "pleamar --scene etta.plm"`: casa com a
  própria linha de comando e mata o shell.
- **Prévia da tela de bloqueio sem travar a sessão:**
  `fact lock.preview true`, ou Settings → Lock → Preview.
- **Recorte de captura simulado:** `fact sel.x/y/w/h …`, depois
  `emit shot_commit`.

Numa sessão na nuvem **não há Halley nem Wayland**. Dá para editar e, se o
pleamar estiver compilado lá, rodar `pleamar --check etta.plm`. Teste visual
só na máquina do usuário.

---

## 4. Estado: o que está feito (commits)

| Commit | Marco |
| --- | --- |
| `0c67b3d`…`294fdc8` | Importação, renomeação Marea→Etta, só inglês, tudo em **Geist** (`family: "Geist"` em cada texto) |
| `846a92d` | Tokens, ícones, kit |
| `d4a4fed` `f7e8dc2` | Relógio e bateria sempre ao lado dela (card com clima, próximos eventos); OSDs (volume, mute, brilho, mic, caps, carregador, bateria baixa, dispositivo, rede, DND) |
| `083008f` | Control center e 13 páginas como componentes; quick settings novos (keep awake = `systemd-inhibit`, power = `powerprofilesctl`, night light = `wlsunset`); Bluetooth levanta o rfkill; volume por app; Settings rolável; Agents com 4 provedores e chips; previsão por hora; History com busca e filtros |
| `62f05b8` | Correções da 1ª revisão: `Ib` não emitia o clique; mídia (pausado também, capa, player); Bluetooth em My devices/Nearby; **sem limite de 60 fps** (judder a 100 Hz); toast e swell redesenhados (mecânica dela mantida) |
| `90afc94` | Guarda-roupa: 5 lugares (`wear.head/face/ears/neck/side`), 20 peças, aba por lugar, preview; acompanhantes à esquerda dela, depois das pedras |
| `7acfab3` | Wallpapers no Halley: field shaders e miniaturas; aplicar escreve `halley-wallpaper.rune`, que o `halley.rune` reúne com `gather` |
| `0c5fd00` | Tela de bloqueio: ela vira o cadeado (pontinhos, `> <` vermelho, `^ ^` com a alça abrindo; destrava 0,9 s depois); prévia segura |
| `4efb5c8` | Captura de região própria (frame congelado, guias, medidas, ela como HUD de W×H; soltar salva em `~/Pictures/Screenshots/Screenshot_…png` + clipboard; Esc cancela; R/W/S trocam o modo); cartão "saved" compartilhado com gravações |
| `0dd7a84` | Look: **Glass bloqueado no Halley** (crash de screencopy); Frosted só translúcido |
| `c7a2f92` | Gravação nativa testada (NVENC + áudio), `~/Videos/Recordings/Recording_…mp4`, rotação de 180° marcada com `ffmpeg -display_rotation` |
| (este marco) | **Estabilidade no Halley** (veja §4.1) e **Finder, menu, cápsula do Haven e "Welcome back"** redesenhados conforme os protótipos |

### 4.1 O que a rodada de estabilidade corrigiu (relatório de testes do usuário)

- **Cliques vazando entre painéis:** o `Seg` declarava `zone box opt.$i`
  dentro de componente. Nomes com `$` perdem o sufixo da instância, então
  todos os `Seg` compartilhavam 5 zonas: "Face" no Wardrobe também escolhia
  Frosted + eDP-1, e qualquer índice 2 ligava "Follow focus". Agora cada
  opção é uma cópia de `SegOpt`.
- **Teclado morto** (Esc, Finder, R/W/S): só a última `keyboard: … while`
  valia (a do lockpreview). Agora há **uma** declaração, na superfície
  principal, `exclusive`. No Halley, `on_demand` só pega o teclado com
  clique/hover numa layer que já é on_demand. Enquanto a Etta segura o
  teclado, os binds do Halley ficam desligados; por isso ela espelha
  Super+Space, Super+Shift+L/S/V e as teclas de volume, brilho e mídia. Uma
  regra `on focus … { blur }` tira o cursor de campos invisíveis.
- **Esc** fecha o que está aberto, de qualquer página. Antes, cancelam:
  senha do Wi-Fi, editor de cidade e confirmação da sessão.
- **Captura:** dentro de superfície nomeada, `pointer.y` vem no plano
  (+10000 por cópia). Cada cópia aprende o próprio deslocamento (`ox`/`oy`)
  pela zona. O overlay abre em **todos** os monitores; o clicado é o
  capturado. Window fica escondido no Halley. Clique direito e Cancel saem.
  O adaptador ganhou `selected` (o output escolhido pelo Halley).
- **Shaders/Deriva:** a zona de rolagem estava por cima dos cards. Agora vem
  antes deles. O 7º shader era inalcançável: o limite da rolagem foi
  arredondado para 0. Agora há paginação "1–6 of 7" com ‹ ›.
- **Settings/Tray/Calendar:** zonas de linhas roladas não respeitam o
  `view`. Agora cada controle só fica ativo com a linha inteira visível, e o
  cabeçalho é desenhado depois da lista.
- **Frame do relógio:** uma transição atrasada sobrevivia a uma mais nova.
  Agora os alvos são `if(cond, 1, 0)`, e o card fica recortado.
- **Prévia do lock:** clicar no fundo não fecha mais. Tem pílula com Close,
  abre no monitor dela e não mostra suspender/desligar.
- **Persistência:** o nome do monitor só é gravado quando você escolhe.
  "View rules" da cápsula não tinha handler e ganhou um.

---

## 5. Mudanças feitas na máquina do usuário (fora do repo)

- **`~/.config/halley/halley.rune`** (backups:
  `halley.rune.bak-before-etta-wallpaper` e `.bak-before-etta-binds`):
  - `gather "~/.local/share/pleamar/etta/halley-wallpaper.rune" as etta_wall`;
    o bloco `wallpaper:` lê `etta_wall.etta.*`;
  - autostart: `once "/home/ely/.local/bin/etta start"` (antes era `marea`);
  - keybinds, com caminho absoluto `/home/ely/.local/bin/etta …`:
    - Super+Space `search`;
    - Super+Shift+L `lock`;
    - Super+Shift+S `shot_region`;
    - Super+Shift+V `record_toggle`;
    - brilho, mídia e mute do microfone.
- **Link** `~/.local/bin/etta` → `~/Work/etta/etta`.
- **A Marea (PID 943) ficou rodando** até o próximo login; o usuário pode
  rodar `marea stop`.
- **Arquivos de teste nas pastas do usuário** (pode apagar):
  - `~/Pictures/Screenshots/Screenshot_2026-10-03_06-54-25.png`;
  - `~/Videos/Recordings/Recording_2026-10-03_06-57-34.mp4`, `06-58-11`,
    `06-58-33` e `07-00-01`.

---

## 6. Armadilhas aprendidas (economizam horas)

**pleamar (cena):**

- `text` com `size`, `weight` e `family` **só aceita literais**. Para variar
  peso ou tamanho, use dois textos com `show`.
- `let` de cor dentro de componente não enxerga outros lets de cor. Ponha o
  `pick()`/`mix()` direto no `color:`.
- `pick()` de texto aceita literais, textos vivos e slots, mas **não** `pick`
  aninhado: faça um índice único com `if()`.
- Textos não entram em expressões (`x != ""` não compila). Use um fact vindo da
  lógica ou um `measure`.
- `measure nome` precisa ser **declarado** (`measure x`) antes de usar
  `measure: x` num texto, inclusive dentro de componentes.
- Um `let` não enxerga facts e props declarados dentro de `repeat`. Use
  `prop … ~16ms` com `follow`.
- Valores de enum não podem colidir com outros nomes. Por isso os prefixos:
  `o_`, `h_`, `f_`, `e_`, `n_`, `s_`, `l_`, `sm_`, `rk_`.
- **`grid` desenha todos os registros possíveis do model** (até o `max`):
  proteja com `show: r.index < model.count`. `column` e `row` com `for` não têm
  esse problema.
- Componentes de biblioteca **não exportam** nomes de zona para a cena: as
  regras `on scroll/press` de uma zona ficam dentro do componente. Nomes de
  layouts em componente também são internos (`list.content` não resolve; use
  conta própria).
- `clip` solto vale até o fim do **grupo**, ou até a próxima `surface` se
  estiver no nível da cena. Sempre embrulhe num `group`.
- `shader x = file "…"` numa biblioteca é relativo à biblioteca
  (`"../shaders/…"`).
- `shader { at: }` é o **canto**; `zone box { at: }` é o **centro**;
  `box { from: }` é o canto.
- Eventos com `->` chegam à lógica; sem a seta, ficam só na cena. Payload na
  regra: `on evento(v) { … }`.
- `on focus` e `on blur` não recebem nome de input.
- Não há cursor crosshair (vocabulário: default, pointer, text, grab,
  grabbing).
- O kind `lock`: o plano é centrado no monitor; `lockscreen.width/height` dá o
  tamanho real.
- Superfícies `screens: each` com regras que leem o ponteiro: abra só no
  monitor alvo (`open: … and screen.index == X`).
- **Nome com `$` dentro de componente é compartilhado por todas as cópias**
  (sem sufixo de instância). Em componente, use nomes fixos ou uma
  subcópia (`SegOpt`).
- **`keyboard: … while` é um só para a cena**: a última declarada vale, e o
  modo é o da superfície principal. Declare só na principal.
- **No Halley use `exclusive`**, porque `on_demand` não recebe o teclado ao
  abrir. Com exclusive, os binds do Halley param: espelhe na cena os que
  importam.
- Ao ganhar o teclado, o pleamar põe o cursor num campo (até num invisível),
  e o campo come letras e Enter antes das regras. `blur` resolve. Enter num
  campo vira submit mesmo com Ctrl.
- **Zonas ignoram `clip` e `view:`**: linha rolada para fora continua
  clicável. Proteja por posição (`live:`/`active:`).
- **Em superfície nomeada, `pointer.x/y` são do plano** (cópia n em
  y + 10000·n); `local.x/y` em regra de zona são corretos.
- **Transição atrasada nunca é cancelada**, e o alvo é lido quando ela cai.
  Escreva `if(cond, 1, 0)`, nunca `1`.
- `on still X for T` dispara **uma vez**: se a guarda estiver falsa nesse
  momento, não dispara de novo. (Foi assim que `face = happy` ficou preso.)
  Prefira gestos (`play …`) para reações curtas.
- O renderer pinta o frame inteiro no Halley. `PLEAMAR_FULL_REPAINT` só
  vale para os frames emprestados do pleamar-wm.
- Mouse simulado (`--mouse`) recebe coordenadas **do plano**: a cópia 0 da
  primeira superfície nomeada fica em y+10000. `--mouse "focus+@… key:Escape@…
  type:abc@…"` simula teclado. Dá para testar componentes reais numa cena
  pequena que importa `ui/`.

**Luau (lógica):**

- O chunk principal está perto do **limite de 200 locais**: código novo vai
  dentro de `;(function() … end)()` ou de uma função nomeada (veja o fim de
  `etta.luau`).
- `files.write` só escreve na pasta de dados da Etta.
- `sys.watch(serviço)` pode ser registrado **uma vez**. Ganchos extras usam a
  tabela `also.network/bluetooth`.
- Numa recarga, a lógica pode subir antes da cena (erro "no fact called X" uma
  vez): toque `etta.luau` de novo.
- `sys.ask("auth.check")` bloqueia a lógica. A cena mostra os estados sozinha.

**Halley:**

- O screencopy gira **duas vezes** monitores com `transform 180`. Captura e
  vídeo do HDMI saem de cabeça para baixo e são corrigidos pela Etta (veja
  `freeze()` e `upside_down`). **Reportar ao Halley.**
- O Glass do pleamar (`lens`) usa `zwlr_screencopy`, e o Halley recusa o buffer
  ("buffer does not match the advertised capture format"). É um erro de
  protocolo e o pleamar sai. **Reportar ao Halley e/ou ao pleamar.**
- O blur de layer-shell no Halley é por regra (`layer-rule namespace`) e vale
  para a superfície inteira. Todas as superfícies do pleamar usam o namespace
  `pleamar`, inclusive o catcher de tela cheia, então **não dá para borrar só o
  painel**.
- A API v1 não expõe a geometria apresentada das janelas: o modo **Window** da
  captura não funciona no Halley.
- O "output focado" do adaptador vem da janela focada. Sem janela focada, a
  Etta cai para o monitor onde mora.
- `halleyctl config verify -c ARQUIVO` valida uma cópia. O Halley recarrega
  sozinho os arquivos reunidos por `gather`.
- O "output selecionado" (o último clicado) vem de `client.trail(None).output`.
  É o que a captura de tela inteira e a gravação usam.
- **Permissões** (`services:`) agora recarregam ao salvar: `clipboard` e
  `clipboard.*` entraram sem reiniciar.

---

## 7. Próximo marco

**Rodada de polimento e otimização**, junto com os resultados do teste ao
vivo deste marco:

- alinhamento fino de todas as páginas, comparando com os protótipos (o
  usuário pediu);
- sensação de fluidez: medir de novo com `etta report`;
- conferir ao vivo o que só o teste real confirma (veja a lista na mensagem
  final do marco): teclado físico no Finder e no menu, Esc em cada página,
  captura nos dois monitores, Ctrl+Enter, "Keep what I copy" (clipboard via
  data-control no Halley) e notificações urgentes atravessando o Haven.

**Valores errados no `settings.json` do usuário** (gravados pelo bug do
`Seg`): `follow: true`, `home: "eDP-1"`, `lock_idle: 2`, `snooze: 2`. O
usuário reescolhe no Settings. Não editar o arquivo sem pedir.

**Backlog conhecido** (fora do marco, anotar e encaixar):

- **Lock when idle:** o adaptador precisa usar `ext_idle_notifier_v1`. O fact
  `idle.k` já existe e é salvo.
- **Caps Lock:** hoje é lido com `cat` a cada 300 ms. Passar para o adaptador.
- **Keep awake:** idealmente via `zwp_idle_inhibit` no adaptador. Hoje é
  logind.
- **Captura de janela no Halley:** dá para delegar ao seletor nativo do
  Halley (`client.capture(CaptureMode::Window, …)` na halley-api) com um
  `etta-halley capture window`.
- **Face presa:** a regra genérica que desfaz `face = happy` (OSD de
  dispositivo/rede, agentes) é de um disparo só. Pode prender a cara feliz
  se o momento passar com outra coisa acontecendo. Tornar robusta.
- **Agents:** o Codex aparece como "No window read yet" quando o rollout mais
  recente não tem `rate_limits`.
- **README e NOTICE:** atualizar com instalação no Halley (keybinds, autostart,
  `gather`).
- **Reportar ao Halley:** a dupla rotação no screencopy, o formato de buffer
  do screencopy e a falta da geometria apresentada das janelas na API v1.
- **Reportar ao pleamar** (textos prontos no diagnóstico desta sessão,
  `~/.cache/etta-dev/diagnosis.md`):
  - nome com `$` em componente sem sufixo de instância;
  - `keyboard: … while` global (a última vence);
  - zonas que ignoram `clip`/`view`;
  - `pointer.x/y` no plano em superfície nomeada;
  - transição atrasada que não é cancelada;
  - campo invisível que pega o cursor ao ganhar o teclado.

## 8. Como o usuário trabalha

- Comunicação em **português**. Gosta de atualizações curtas durante trabalhos
  longos.
- Mudanças em `halley.rune` e em qualquer config dele: **mostrar o trecho
  antes**. A estrutura de wallpaper com `gather`, os keybinds e o autostart já
  foram aprovados e aplicados.
- Valida tudo ao vivo e volta com uma lista de inconsistências.
- O repositório remoto é **privado**: `github.com/ely-id/etta`.
