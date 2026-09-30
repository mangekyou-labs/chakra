# Chakra — 3-minute Circle grant demo script

Workspace: `feature-chakra`. Product: **Chakra** on Arc Testnet. Wallet: **MetaMask**.
Proposed ask: Circle Developer Grants 2026 Cohort 2 (40,000 USDC over 20 weeks, four milestones). This is an application, not an award.

Questbook field: **Video demo URL** (currently `TO_BE_FILLED` in
[circle-developer-grant-proposal.md](circle-developer-grant-proposal.md)).
Requirements: technical video **≤5 minutes**, **codebase walkthrough**,
**integration demonstration**. This cut is roughly **3 minutes 15 seconds**.

Use the accepted four-milestone application skeleton and this short cut when
preparing the recording. Keep room within the five-minute limit for wallet delays.

A still image is not this video. Use a Chakra screenshot as the YouTube
*cover* on top of the recording. Do not paste a PNG into Questbook’s video field.

---

## How to record

Speak **English** on camera. Do not mention language. Do not apologize for
accent. Reviewers are Circle staff.

**Preferred:** record the screen silent, then dub this script in QuickTime or
CapCut. ESL voiceovers beat live “um” during MetaMask. Second-best: live
voice with this script in 48pt font on a second monitor.

- About 90 words per minute. Short sentences. Half-second pause after each
  period.
- Face optional. Screen is the product. No music. No zoom spam.
- 1920×1080, English OS, bookmarks hidden.
- Click, then talk. Do not hunt the mouse while speaking.
- If you misspeak, pause two seconds and say the sentence again. Do not
  restart the whole take unless the swap failed.

Text in `[brackets]` is an action. Do not read it. **Meaning** is rehearsal
only — do not speak it on the recording.

---

## MetaMask dead time (not your internet)

40 MB/s download will not stall this demo. These will:

| Wait | Typical | Worst | What you do |
| --- | --- | --- | --- |
| Render API cold start (`chakra-api-0a5i.onrender.com`) | 1–3s if warm | 15–40s | Pre-warm `/health` + `/quote` 2 minutes before record |
| Quote “Finding route…” | 0.5–2s | 5s | Amount `1` already entered; quote already on screen |
| Unaudited modal | click | — | Ack **before** record |
| Connect / Switch to Arc | 5–20s | hung popup | Wallet already connected; green chip already on Arc Testnet |
| First-time `Approve USDC` (on-chain) | 10–25s | 40s+ | If a live route exists, do a small preparation swap before recording so Permit2 allowance exists |
| `Sign Permit2` (typed data, no chain wait) | 3–8s | you froze | Start the Permit2 paragraph **as the popup opens** |
| `Sending…` + `Confirming…` (`splitSwap` + 1 receipt) | 8–20s | 40s+ Arc testnet | Start the splitSwap paragraph immediately; use stretch lines |
| Failed / rejected popup | — | take dies | Backup tab: dated single-route receipt `0xee7b…da6c`. Do not sit in silence. |

**Pre-approve when filming a fresh swap.** A first-time swap is three
MetaMask steps (approve tx + Permit2 sign + splitSwap tx) and can eat 45–70
seconds.

Position MetaMask so the popup is **visible on the right**. Reviewers need
to see a human signing. Do not hide it off-screen.

Button labels from `SwapCard.tsx`, in order: `Building transaction…` →
`Approve USDC` (skip if pre-approved) → `Sign Permit2` → `Sending…` →
`Confirming…` → `Confirmed ✓`. Success banner: `Swap confirmed!` +
`View on Arcscan`.

---

## Pre-flight (do once, not on camera)

1. Close Slack, Telegram, email, extra Chrome profiles.
2. Warm the API:

```bash
curl -sS https://chakra-api-0a5i.onrender.com/api/v1/health
curl -sS 'https://chakra-api-0a5i.onrender.com/api/v1/quote?token_in=0x3600000000000000000000000000000000000000&token_out=0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a&amount_in=1000000&slippage=0.5'
```

3. MetaMask unlocked. Arc Testnet (`5042002`). Native USDC gas + ERC-20 USDC
   + EURC balances. Enough gas.
4. Open https://chakra-ag.vercel.app — already connected, green chip,
   unaudited already accepted.
5. Confirm a current USDC→EURC route exists. If it does not, show the honest
   no-route state and use the dated receipt as historical evidence; do not
   imply a fresh swap is possible.
6. If this wallet has never approved Permit2 on this aggregator and a route
   exists, make a small preparation swap and wait until `Confirmed ✓`.
   This is controlled testnet QA, not customer activity.
7. Enter **1** USDC → EURC. Wait until Route / Price impact / Protocol fee
   are visible. Record the displayed venue and EURC amount for the spoken line.
   Do not click Swap yet.
8. Tabs, left to right. Prefer **local VS Code in this worktree** for code
   (GitHub default branch may lag `feature-chakra`):
   - Swap (live quote)
   - `docs/arc-testnet-manifest.json` (chain ID, USDC, EURC, aggregator)
   - `contracts/evm/src/Aggregator.sol` line 209 `function splitSwap`
   - `crates/api-server/src/build_tx.rs` (`build_tx_data`, Permit2 typed data)
   - `packages/sdk/src/index.ts` (`quote` and `buildTx`; no Circle adapter)
   - Backup explorer: https://explorer.testnet.arc.io/tx/0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c
9. Check that the backup explorer still shows success and the documented
   aggregator. Make a 10-second mic test. Then record.

Live URLs if a reviewer asks later:

- UI: https://chakra-ag.vercel.app
- API: https://chakra-api-0a5i.onrender.com
- Repo: https://github.com/mangekyou-labs/chakra
- Aggregator: `0xeb12351602c56d47c4ee955193335848952b29d8`

---

## Pronunciation (practice twice)

| Word | Say |
| --- | --- |
| Chakra | **CHAH-kruh** |
| Arc | **Ark** |
| USDC | **U-S-D-C** |
| EURC | **E-U-R-C** |
| Xylo | **ZY-lo** |
| Presto | **PRES-toh** |
| UnitFlow | **YOU-nit-flow** |
| Permit2 | **PER-mit two** |
| MetaMask | **MET-uh-mask** |
| Arcscan | **ARK-scan** |
| calldata | **call-day-tuh** |
| aggregator | **AG-ruh-gay-tor** |
| CCTP | **C-C-T-P** |

Do not say “tranche.” Say “this grant.” Do not say “non-custodial.” Say “we
never hold keys.” Do not say “differentiation.” Do not say atomic amounts
(`1000000`). Say **one USDC**.

---

## Teleprompter

### 0:00–0:12 — Open (~16 words)

**[Show chakra-ag.vercel.app. Cursor still.]**

**Say:**

Hello. This is Chakra. A DEX aggregator on Arc Testnet. Your wallet signs.
We never hold keys.

**Meaning:** Đây là Chakra, bộ định tuyến DEX trên Arc Testnet. Ví ký.
Chúng tôi không giữ khóa.

If you have two extra seconds, add: “It compares supported USDC and EURC
venues when liquidity is available.”

### 0:12–0:48 — Live quote (~49 words)

**[Point slowly: Sell USDC → Buy EURC → amount 1 → Price impact → Protocol
fee → Route. Open the Route row if it is collapsed.]**

**Say:**

Here is a live quote. One USDC quotes to [read current EURC amount] through
[read current venue]. Price impact and protocol fee are on screen. Gas on Arc
is native USDC. This is a testnet pool price, not a euro-to-dollar rate.

**Meaning:** Báo giá trực tiếp. Đọc số EURC và venue đang hiển thị. Phí giao thức
và trượt giá hiện trên màn hình. Gas trên Arc là USDC gốc. Đây là giá pool
testnet, không phải tỷ giá EUR/USD thật.

Replace both bracketed slots with what the screen shows; do not read the
brackets. Do not memorize a venue or invent an amount. If there is no route,
say: “There is no current USDC-to-EURC route, so Chakra disables execution.
I will show a dated testnet swap receipt instead.” Then skip to **Code
walkthrough** after showing the historical receipt.

### 0:48 — Click Swap

**[Only with a current route, click Swap. Button becomes Building
transaction… then Sign Permit2. MetaMask typed-data popup.]**

If the button says **Approve USDC**, you skipped pre-approve. Stop the take.
Do the 0.1 USDC swap, then record again.

### 0:48–1:12 — Dead time A: Permit2 popup (~37 words)

**[Do not stare at the popup in silence. Talk, then click Sign.]**

**Say:**

The API built an unsigned transaction. MetaMask now asks for a Permit2
signature. Permit2 lets the aggregator pull only this swap amount. Chakra
does not submit. The wallet submits.

**[Click Sign.]**

**Meaning:** API chỉ tạo giao dịch chưa gửi. MetaMask xin chữ ký Permit2 để
aggregator kéo đúng số USDC của lệnh này. Chakra không gửi tx. Ví mới gửi.

### 1:12–1:38 — Dead time B: Sending / Confirming (~39 words)

**[Button: Sending… then Confirming…. Second MetaMask popup if it asks to
confirm the tx — click Confirm immediately, then keep talking.]**

**Say:**

Next, MetaMask confirms the on-chain swap. One contract call. Function name
split swap. Output returns to this wallet in the same transaction. If the
quote is stale, the contract stops.

**Meaning:** MetaMask xác nhận swap on-chain. Một lời gọi hợp đồng
`splitSwap`. Tiền ra về ví này trong cùng giao dịch. Quote cũ thì contract
dừng.

**Stretch (only if still Confirming… after those lines):**

Venues today are Xylo, Presto, and UnitFlow. We do not seed liquidity to
fake a split.

**Meaning:** Các sàn hiện tại: Xylo, Presto, UnitFlow. Chúng tôi không bơm
thanh khoản để giả split.

### 1:38–1:52 — Receipt (~14 words)

**[Wait for green “Swap confirmed! View on Arcscan”. Click it. Hover method
`splitSwap` and status ok. Do not scroll the whole log.]**

**Say:**

Confirmed. This is the public receipt on Arcscan. Method split swap. Status
ok.

**Meaning:** Đã xác nhận. Biên lai công khai trên Arcscan. Hàm splitSwap.
Status ok.

### 1:52–2:22 — Code walkthrough (required) (~48 words)

**[Show `docs/arc-testnet-manifest.json` chain and token addresses;
`contracts/evm/src/Aggregator.sol` `splitSwap`; `crates/api-server/src/build_tx.rs`
`build_tx_data`; then `packages/sdk/src/index.ts` `quote` and `buildTx`.
Keep each view brief. Avoid the SDK README's outdated transaction wording.]**

**Say:**

This manifest names Arc Testnet, USDC, EURC, and the deployed aggregator.
The contract uses Permit2 and checks minimum output. Build transaction
encodes unsigned calldata; the wallet sends it. The in-repo SDK can quote
and build, but has no Circle wallet adapter yet.

**Meaning:** Manifest ghi chain testnet, token và aggregator. Contract dùng
Permit2 và kiểm tra đầu ra tối thiểu. `build_tx` mã hóa calldata chưa ký;
ví gửi giao dịch. SDK hiện có quote/build, chưa có Circle wallet adapter.

### 2:22–2:40 — Stats, honest (~24 words)

**[Optional: tab to https://chakra-ag.vercel.app/stats. Do not overclaim numbers.]**

**Say:**

These are controlled testnet swaps, not organic customer volume. A mined
two-venue split receipt remains pending.

**Meaning:** Đây là swap QA trên testnet, không phải volume người dùng thật.
Chưa có biên lai on-chain cho split hai venue.

### 2:40–3:02 — Planned grant work (required honesty) (~33 words)

**[Show the four-milestone application outline. The in-repo SDK shown above
does not yet produce a Circle-wallet payload.]**

**Say:**

Four proposed jobs: publish the SDK and Circle adapter; prove a user-controlled
smart-account swap; prove separate Gas Station sponsorship; then prove inbound
CCTP USDC. None is shipped. Arc mainnet swap proof is self-funded.

**Meaning:** Đề xuất bốn phần việc: xuất bản SDK với adapter Circle wallet,
chứng minh swap bằng smart account, swap được tài trợ gas riêng, rồi CCTP
đưa USDC vào Arc. Các luồng Circle chưa được triển khai. Bằng chứng swap
Arc mainnet tự tài trợ, ngoài đề nghị này.

### 3:02–3:14 — Close (~17 words)

**Say:**

Our proposed Circle request is forty thousand USDC over twenty weeks.
Each milestone would have public evidence. Thank you.

**Meaning:** Đề nghị Circle là 40,000 USDC trong 20 tuần. Mỗi cột mốc có
bằng chứng công khai. Cảm ơn.

A 3:15 take is fine. Questbook allows five minutes; reserve the extra time
for a slow wallet or explorer.

---

## If MetaMask hangs (do not freeze)

At 0:48, if the popup never comes or you click Reject:

**[Switch to the backup Arc explorer tab.]**

**Say:**

I will not wait on a new wallet prompt. Here is a public MetaMask receipt
from September fifth. One USDC to one point six two nine EURC on Presto.

Then continue at **1:52 Code**.

Label this as a historical controlled-QA receipt, never a fresh swap or
Circle-wallet integration. A fresh successful receipt is stronger.

Backup receipt:
https://explorer.testnet.arc.io/tx/0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c
(1 USDC → 1.629188 EURC on Presto, 5 September 2026, block 60563600, method
`splitSwap`).

---

## Cut order if the take runs long

1. Drop the stretch venue sentence.
2. Drop the optional stats segment. The receipt and code remain the proof.
3. Keep code + planned grant work + close. Those are the grant-reviewer lines.

If the take is under 2:30, you went too fast or skipped code. Do another
take.

---

## Do not say

- “MetaMask is slow because of my internet.”
- “Sorry for my English.”
- “We already have Circle Wallets / Gas Station / a Circle-wallet SDK.”
- “We are live on Arc mainnet.” (This recording demonstrates Arc Testnet.
  The separate mainnet proof needs its own dated receipt and live route.)
- Vault, round-trip, or operator bot. This video shows Chakra’s Circle embed path.
- “We have split swaps in production.” (`split_swaps` was 0 at the 5 Sep
  check.)
- “1 USDC → 1,636,332 EURC” (atomic misread).
- “This is euro dollar FX.”
- A mined split, Circle-authorized swap, sponsored swap, CCTP transfer,
  customer adoption, or grant award without matching dated evidence.
- Private keys, seed phrases, `.env`, Circle API tokens.

---

## After the take

1. Watch once without looking at the script. If you cannot hear a sentence,
   re-dub that sentence.
2. Upload **unlisted YouTube** or Drive (anyone-with-link). Title:
   `Chakra — Circle Developer Grant demo (Arc Testnet)`.
3. Incognito-check the URL.
4. Paste into Questbook **Video demo URL**.
5. Prepare a transcript that marks USDC/EURC as current and Circle Wallets,
   Gas Station, and CCTP as planned. Give the real code locations shown.
6. Capture code screenshots only for current token, gas, transaction, and
   wallet paths. Do not create screenshots for planned Circle integrations.
7. Optional YouTube thumbnail: a Chakra UI screenshot (USDC→EURC + route).
