# Chakra - Arc DEX Aggregator

## 项目概述

**Chakra** 是 Arc 生态的 DEX 聚合器，聚合 6 个 DEX 的流动性，为用户提供最优 swap 路由。

- **网站**: https://Chakra.xyz
- **API**: https://api.Chakra.xyz
- **合约**: `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`
- **GitHub**: https://github.com/ligulfzhou/Arc-dex-agg (private)
- **服务器**: 178.63.81.216 (API + Arc RPC + Horizon)

---

## 架构

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│  Frontend   │────▶│  API Server  │────▶│  Arc RPC    │
│  (Next.js)  │     │  (Rust/Axum) │     │  (178.63.81.216)│
│  Cloudflare │     │  Port 3100   │     │  Port 8003      │
└─────────────┘     └──────────────┘     └─────────────────┘
                           │
                    ┌──────┴──────┐
                    │ Router Engine│
                    │ - PathFinder │
                    │ - QuoteEngine│
                    │ - SplitOptim │
                    └──────┬──────┘
                           │
          ┌────────────────┼────────────────┐
          │                │                │
    ┌─────┴─────┐   ┌─────┴─────┐   ┌─────┴─────┐
    │ Arc venue  │   │ Arc venue  │   │  Sushi V3 │
    │ 192 pools │   │ 278 pools │   │  1+ pools │
    └───────────┘   └───────────┘   └───────────┘
          │                │                │
    ┌─────┴─────┐   ┌─────┴─────┐   ┌─────┴─────┐
    │  Arc venue  │   │   Arc venue   │   │Classic DEX│
    │  11 pools │   │  1 pool   │   │  3 pairs  │
    └───────────┘   └───────────┘   └───────────┘
```

---

## 已完成功能 ✅

### 核心引擎
- [x] **6 个 DEX Adapter**: Arc venue, Arc venue (volatile+stable+CLMM), Arc venue, Sushi V3, Arc venue, Classic DEX
- [x] **CLMM 本地计算**: 纯 Rust Uniswap V3 tick math，验证 0% 误差
- [x] **Arc venue (Balancer V1) 本地计算**: 加权池数学
- [x] **StableSwap math**: Curve N-token invariant（已实现，待校准）
- [x] **Split Routing**: Brent's method 优化，0.1% impact 阈值触发
- [x] **多跳路由**: 自动发现 A→B→C 路径（最多 3 跳）
- [x] **Price Impact 计算**: `1 - actual_out / ideal_out`
- [x] **Reserves 批量刷新**: getLedgerEntries 每 5 秒刷新

### 聚合合约 (Arc)
- [x] **已部署到 mainnet**: `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`
- [x] **支持 5 种 DEX**: Arc venue, Arc venue, Arc venue, Sushi, Arc venue
- [x] **swap()**: 单路径多跳
- [x] **split_swap()**: 分单执行
- [x] **upgrade()**: 合约可升级（admin only）
- [x] **initialize()**: admin 设置

### API 端点
- [x] `GET /api/v1/quote` — 获取最优路由（含 pool_addresses, dex_types）
- [x] `POST /api/v1/build_tx` — 构建调用聚合合约的交易（含 simulate）
- [x] `GET /api/v1/tokens` — 返回所有 236 个 token（235 个有名字）
- [x] `GET /api/v1/health` — 健康检查

### 前端
- [x] Swap UI（输入金额自动报价）
- [x] Token selector（全屏 modal，搜索，logo）
- [x] 路由显示（DEX 来源、百分比、price impact）
- [x] 钱包连接（wallet/xBull/LOBSTR）
- [x] Swap 执行流程（build_tx → 签名 → 提交）
- [x] API Docs 页面（交互式 Try It）
- [x] 导航（Swap / API Docs）

### 部署
- [x] API 服务器 systemd service
- [x] Nginx reverse proxy + Cloudflare Origin Certificate
- [x] 前端 Cloudflare Pages
- [x] 部署脚本 `deploy_server.sh`（rsync + 服务器编译）

---

## 已知问题 / 待修复 🔧

### 高优先级

| 问题 | 描述 | 状态 |
|------|------|------|
| 3-token pool | Arc venue 的 XLM/USDC/AQUA 3-token stableswap pool 被 blacklist 了。stable_math 模块已写好，但需要从链上读取正确的 amp 值和 token 顺序来校准 | 暂时跳过 |
| Sushi pool 发现 | 服务器只发现 1 个 Sushi pool（应该 20 个）。hardcoded 的 pool 地址在服务器 RPC 上调用失败 | 服务器 RPC 兼容性问题 |
| build_tx simulate | simulate 流程已实现，但如果 pool 地址不对会报 EmptyPool 错误。前端需要正确传递 quote 返回的 pool_addresses | 基本工作 |

### 中优先级

| 问题 | 描述 |
|------|------|
| Arc venue CLMM 加载慢 | 需要对 321 个 pool 调用 pool_type() 过滤 concentrated，启动时间长 |
| Arc venue adapter | 可能因 RPC 问题未正确加载 |
| Token metadata | 框架搭好了（token_metadata.rs），但服务器上 resolve_unknown 没有执行成功 |
| 前端 swap 执行 | build_tx 返回的交易可能因 pool 问题导致 simulate 失败 |

### 低优先级

| 问题 | 描述 |
|------|------|
| 更多 token logo | 目前只有 XLM/USDC/EURC/AQUA 有 logo |
| 汇率显示 bug | 前端 "1 XLM ≈ xxx USDC" 计算有时显示异常大的数字 |
| Classic DEX impact | 用固定值估算（假设 $1M 流动性），不够精确 |

---

## 精度验证结果

### Arc venue Concentrated (CLMM)
| Pool | 本地计算 | 链上结果 | 差异 |
|------|---------|---------|------|
| CA4HTZ... | 465,073,372,612 | 465,080,500,184 | 0.0015% |
| CADMDT... | 999,535,041 | 999,535,041 | **0%** |
| CBBMQB... | 148,093,752 | 148,093,752 | **0%** |

### Sushi V3 (CLMM)
| 金额 | 本地计算 | 链上结果 | 差异 |
|------|---------|---------|------|
| 1 XLM | 1,474,747 | 1,474,747 | **0%** |
| 10 XLM | 14,747,471 | 14,747,471 | **0%** |
| 100 XLM | 147,474,169 | 147,474,169 | **0%** |
| 1000 XLM | 1,474,686,861 | 1,474,686,861 | **0%** |
| 10000 XLM | 14,741,387,788 | 14,741,387,788 | **0%** |

---

## 技术栈

| 组件 | 技术 |
|------|------|
| 后端 | Rust, Axum, Tokio |
| 前端 | Next.js 15, React 19, Tailwind CSS 4 |
| 合约 | Arc (Rust, Arc-sdk) |
| 部署 | Cloudflare Pages (前端), systemd (API) |
| RPC | 自建 Arc Core + Arc RPC |

## 代码结构

```
Arc-dex-aggregator/
├── contracts/aggregator/       # Arc 聚合合约
├── crates/
│   ├── api-server/            # Axum API 服务器
│   ├── dex-adapters/          # 6 个 DEX adapter
│   │   ├── Arc venue.rs        # Arc venue (volatile + stable)
│   │   ├── Arc venue_clmm.rs   # Arc venue concentrated
│   │   ├── Arc venue.rs        # Arc venue
│   │   ├── Arc venue.rs         # Arc venue
│   │   ├── sushi.rs           # Sushi V3
│   │   ├── Arc venue.rs           # Arc venue (Balancer)
│   │   ├── classic_dex.rs     # Arc Classic DEX
│   │   ├── clmm_math.rs      # CLMM tick math (shared)
│   │   ├── Arc venue_math.rs     # Balancer V1 math
│   │   ├── stable_math.rs    # Curve StableSwap math
│   │   └── batch_refresh.rs  # getLedgerEntries batch
│   └── router-engine/        # 路由引擎
│       ├── quote_engine.rs    # 报价编排
│       ├── path_finder.rs     # BFS 路径发现
│       ├── split_optimizer.rs # Brent's method split
│       └── graph.rs           # Token graph
├── packages/frontend/         # Next.js 前端
└── thirdparty/                # 第三方合约源码参考
    ├── Arc venue-amm/
    ├── Arc venue-contracts-v1/
    └── sushiswap-Arc-interface-fork/
```

---

## 部署流程

```bash
# 前端部署（Cloudflare Pages）
cd packages/frontend
npm run build
npx wrangler pages deploy out --project-name=Chakra

# API 部署（服务器编译）
./deploy_server.sh
# 或手动：
rsync -az --exclude target --exclude .git --exclude thirdparty crates/ root@178.63.81.216:/opt/Arc-dex-aggregator-src/crates/
ssh root@178.63.81.216 "source ~/.cargo/env && cd /opt/Arc-dex-aggregator-src && cargo build --release -p api-server && systemctl stop Chakra-api && cp target/release/api-server /opt/Arc-dex-aggregator/target/release/api-server && systemctl start Chakra-api"
```

---

## 关键数据

- **总 Pools**: ~489 (Arc venue 192 + Arc venue 278 + Arc venue 11 + Sushi 1 + Classic 3 + Arc venue 1)
- **总 Tokens**: 236
- **报价速度**: <1ms
- **Split 触发**: ~100K XLM ($14K) 以上
- **Reserves 刷新**: 每 5 秒
- **合约 WASM**: 14.7 KB (optimized)
- **部署成本**: ~3-5 XLM 一次性 + ~6 XLM/年租金

---

*Last updated: 2026-05-19*
