# Rust 中文检索方案选型

> 研究工单：[#3 Rust 中文检索方案选型](https://github.com/zibyn/almanac/issues/3)（地图：[#1](https://github.com/zibyn/almanac/issues/1)）
> 调研日期：2026-10-20。版本号、下载量取自 crates.io API 当日数据。
> 参考资料，决策以对应 ticket 的结论为准。

## 问题

不依赖外部服务的前提下，Rust 生态里哪些方案可以做中文及中英混排（含代码标识符）的确定性词法检索（BM25 类 + 结构化过滤）？
比较维度：二进制/词典体积、分词准确度、新词与代码标识符处理、索引构建与查询性能、可重建性。
约束（来自 #1）：单人库，规模为数千到数十万条；不依赖外部数据库/向量库；索引是派生物，可由只追加事件日志重建；同一核心既有 CLI 单次调用，也有常驻 MCP / Server 进程。

## 结论先行

1. **问题拆成两层**：分析器（文本 → 词元）决定中文与代码标识符的召回质量；索引引擎（倒排 + BM25 + 持久化）决定性能与可重建性。两层可独立选择，中间的缝是"文本 → 词元序列"这个纯函数。
2. **分析器推荐：自写一个薄的组合分析器**——汉字段交给 `jieba-rs` 搜索模式（`cut_for_search`），ASCII 段用自写的标识符拆分（保留整体 + 拆出 camelCase / snake_case / `::` / `.` 各部分，小写化），另开一个 CJK 二元组（bigram）字段兜底新词。现成的 `tantivy-jieba`、`cang-jie` 只做 jieba 一层，不处理 camelCase；`charabia` 处理得最全但依赖重、为 Meilisearch 定制。
3. **索引引擎推荐：`tantivy`**，自写倒排作为备选。tantivy 自带 BM25、短语查询、mmap 持久化、<10ms 启动、快速字段（可做结构化过滤），重建 = 删目录 + 重放日志；代价是依赖体积较大、单写者锁（`.tantivy-writer.lock`）。对 CLI 每次启动都要可用的场景，持久化索引比"每次启动从日志重建内存索引"更现实。
4. **不推荐**：纯 n-gram（tantivy `NgramTokenizer` 的 position 恒为 0，无短语查询，且 ASCII 也会被切成 n-gram）；SQLite FTS5 trigram（少于 3 个字符的查询不命中，而中文常用词多为 2 字）；UAX #29 / `unicode-segmentation`（汉字逐字切分，等同 unigram）。
5. **实测要点**（jieba-rs 0.11）：剥离后最小程序 3.5 MB，`Jieba::new()` ≈86 ms，`cut_for_search` ≈90 MiB/s；"缺字段"被误切为 `缺字` `段`、"智能体"被切成 `智能` `体`——佐证需要 bigram 兜底字段和用户词典；CLI 应惰性加载词典。
6. **可重建性的关键不在引擎而在分析器版本**：jieba 的结果由"内置词典 + 用户词典 + HMM 开关 + crate 版本"共同决定。索引元数据必须记录分析器指纹，指纹变化即触发全量重建；用户词典应作为配置/事件进入日志。

## 候选方案一览

| 方案 | 类型 | 最新版本 / 更新 | 中文处理 | 代码标识符 | 词典 / 体积 | 可扩展新词 | 备注 |
|---|---|---|---|---|---|---|---|
| [`jieba-rs`](https://github.com/messense/jieba-rs) | 分词器 | 0.11.0 / 2026-09 | 前缀词典 DAG + 动态规划，未登录词走 HMM | 汉字块内保留 ASCII 字母数字及 `+#&._%-`；搜索模式会把 `WES-5.4.5` 拆出 `WES` | 内嵌 `dict.txt` 5.07 MB / 34.9 万词条（zstd 压缩后嵌入）；实测最小程序剥离后 3.5 MB，`Jieba::new()` ≈86 ms | `add_word` / `load_dict` 运行时加用户词典 | 事实上的 Rust 中文分词标准实现，下载量 468 万 |
| [`tantivy-jieba`](https://github.com/jiegec/tantivy-jieba) | tantivy 适配器 | 0.21.0 / 2026-10 | 同 jieba-rs，默认搜索模式 + HMM | 同 jieba-rs | 同 jieba-rs | `with_jieba(自定义实例)` | 全局实例用 `lazy_static` 首次分词时加载词典 |
| [`cang-jie`](https://github.com/DCjanus/cang-jie) | tantivy 适配器 | 0.21.0 / 2026-09 | jieba 的 All / Default / ForSearch / Unicode(逐字) 四种模式 | 同 jieba-rs | 同 jieba-rs | 传入自定义 `Jieba` | 比 tantivy-jieba 多一个逐字模式 |
| [`tantivy`](https://github.com/quickwit-oss/tantivy) | 全文检索引擎库 | 0.27.0 / 2026-10 | 内置 `SimpleTokenizer` 按 `is_alphanumeric` 切分——整段汉字成一个词元，不可用；需第三方分词器 | 需自写 | 引擎本身不带词典；crate 包 1.25 MB，依赖树大 | 由分词器决定 | BM25、短语查询、mmap、快速字段、增量索引、<10ms 启动 |
| [`charabia`](https://github.com/meilisearch/charabia) | 多语言分析器 | 0.10.0 / 2026-08 | 内部用 jieba（不开 HMM），另做 kvariant 归一化、可选拼音 | 可选 `latin-camelcase`、`latin-snakecase` 特性 | 默认特性还会嵌入日文 UniDic、韩文 ko-dic 等词典，需手动裁剪特性 | 不直接暴露 jieba 实例 | Meilisearch 的分词库；中文分词约 10 MiB/s、完整 tokenize 约 5 MiB/s（官方表） |
| [`lindera`](https://github.com/lindera/lindera) | 形态分析器（MeCab 系） | 6.2.0 / 2026-09 | 可嵌入 `embed-cc-cedict` 或 `embed-jieba` 词典 | 无专门处理 | 视所选词典而定 | 支持用户词典 | 主战场是日文/韩文；中文不如直接用 jieba-rs |
| [`icu_segmenter`](https://github.com/unicode-org/icu4x) | Unicode 分词 | 2.3.0 / 2026-08 | `new_auto` 对中日文用内置字典模型 | 无 | 编译进数据，体积大 | 不支持自定义词 | 标准化但不可调，不适合需要领域词的场景 |
| [`unicode-segmentation`](https://crates.io/crates/unicode-segmentation)（UAX #29） | Unicode 词边界 | — | 汉字逐字切分 | 不拆 camelCase | 无词典 | — | [`bm25`](https://github.com/Michael-JB/bm25) crate 的默认分词器即基于它 |
| CJK n-gram（自写或 tantivy `NgramTokenizer`） | 无词典切分 | — | 双字/三字滑窗，召回高、精度低 | tantivy 的实现会把 ASCII 也切成 n-gram | 0 | 天然覆盖新词 | tantivy 的 `NgramTokenizer` position 恒为 0，无法做短语查询 |
| SQLite FTS5（`rusqlite` bundled） | 内嵌数据库全文检索 | — | `unicode61` 不切中文；`trigram` 做子串匹配 | trigram 可子串匹配 | 0 | — | 少于 3 个 Unicode 字符的子串在全文查询中不命中任何行 |
| [`bm25`](https://github.com/Michael-JB/bm25) | BM25 嵌入/打分/内存搜索 | 2.3.2 / 2025-09 | 默认分词器无中文支持，可实现 `Tokenizer` trait 接入 jieba | 自定义 | 0 | — | 内存引擎、无持久化；可借用其打分器 |

> 版本/日期来源：`https://crates.io/api/v1/crates/<name>`。

## 逐项分析

### 1. 分词器：jieba-rs 是中文这一层唯一靠谱的选择

- **算法与准确度**：jieba 基于前缀词典构建 DAG，用动态规划取最大概率路径，未登录词用 HMM（Viterbi）识别（[`jieba/src/lib.rs`](https://github.com/messense/jieba-rs/blob/main/jieba/src/lib.rs)、[`hmm.rs`](https://github.com/messense/jieba-rs/blob/main/jieba/src/hmm.rs)）。它是 Python jieba 的移植，词典与模型同源（`dict.txt`、`hmm.model`）。没有找到针对 jieba-rs 的独立分词准确率评测；对于检索而言，"搜索模式 + 二元组兜底"比追求切分精度更重要。
- **搜索模式**：`cut_for_search` 在常规切分结果上，再把长词中属于词典的 2 字、3 字子串也输出（实测："后台任务"同时输出 `后台` `任务` `后台任务`，"数据库"输出 `数据` `据库` `数据库`），提高召回（源码 `cut_for_search`）。
- **中英混排与代码标识符**：汉字块的字符类 `RE_HAN_DEFAULT` = CJK + ASCII 字母数字 + `+#&._%-`，所以 `append_only_log`、`config.yaml`、`v0.3.5` 会作为整体保留；0.11 起搜索模式还会按连接符 `.` `_` `-` 拆出长度 ≥2 且含字母的部分（源码注释：“`WES-5.4.5` is findable as `WES` and `5.4.5` as well as whole”）。**但它不拆 camelCase**（`getUserById` 仍是一个词元），也不把 `::`、`/` 当连接符——这部分需要自写。
- **新词**：`add_word(word, freq, tag)`、`load_dict(reader)` 可在运行时增加用户词典；HMM 可识别部分未登录词，但不稳定。
- **体积**：默认特性 `default-dict` 用 `include-flate` 以 zstd 压缩方式内嵌 5.07 MB 的 `dict.txt`；HMM 模型 520 KB 由 `jieba-macros` 在编译期生成。可关掉 `default-dict` 改为运行时从文件加载（`Jieba::with_dict`）。
- **性能**：维护者的优化记录称比 cppjieba 快约 33%，后续又提速 2.4 倍（[博文 1](https://blog.paulme.ng/posts/2019-06-30-optimizing-jieba-rs-to-be-33percents-faster-than-cppjieba.html)、[博文 2](https://messense.me/making-jieba-rs-2-4x-faster)）；charabia 官方表给出其中文分词约 10 MiB/s（含其自身管线开销）。本地实测 `cut_for_search` 约 90 MiB/s，词典加载约 86 ms，见下文。

### 2. tantivy 的适配器：tantivy-jieba 与 cang-jie

两者都只是把 `jieba-rs` 包成 tantivy 的 `Tokenizer`（[tantivy-jieba `src/lib.rs`](https://github.com/jiegec/tantivy-jieba/blob/master/src/lib.rs)，cang-jie 的 `TokenizerOption`：All / Default{hmm} / ForSearch{hmm} / Unicode）。tantivy-jieba 默认 `search_mode: true, hmm: true`，全局实例用 `lazy_static` 延迟初始化。二者都不做 camelCase 拆分与小写化（需叠加 tantivy 的 `LowerCaser` 过滤器）。它们代码量很小，almanac 自写一个组合分析器并实现 [`tantivy-tokenizer-api`](https://crates.io/crates/tantivy-tokenizer-api) 的 `Tokenizer` trait 即可，没必要依赖它们（还可避开 jieba-rs 版本被适配器锁定的问题）。

### 3. 索引引擎：tantivy vs 自写倒排

**tantivy**（[README](https://github.com/quickwit-oss/tantivy)）：Lucene 风格，BM25（与 Lucene 相同）、短语查询、增量索引、mmap 目录、快速字段（u64/i64/f64/日期/bool/facet，可做 Scope、Writer、时间等结构化过滤）、"Tiny startup time (<10ms), perfect for command-line tools"。注意点：

- **单写者**：同一索引目录同一时刻只能有一个 `IndexWriter`，靠 `.tantivy-writer.lock` 实现；进程崩溃后残留的锁文件可手动删除（[`directory_lock.rs`](https://github.com/quickwit-oss/tantivy/blob/main/src/directory/directory_lock.rs)）。读者可以在另一进程并发读。almanac 的 CLI / MCP / Server 若可能同时写，需要在"追加日志 → 更新索引"外层自己串行化（日志追加本来也需要锁）。
- **写入内存**：每线程内存预算下限 `MEMORY_BUDGET_NUM_BYTES_MIN`（约 15 MB，见 [`index_writer.rs`](https://github.com/quickwit-oss/tantivy/blob/main/src/indexer/index_writer.rs)）。
- **依赖体积**：0.27.0 的 `Cargo.toml` 列出 53 个依赖项（含可选），默认特性开启 mmap、lz4、zstd 列存压缩、stemmer、stopwords；会显著拉长编译时间、增大二进制。本次未实测（网络下载太慢），可关闭默认特性只留 `mmap` 来瘦身。
- **可重建**：索引就是一个目录，删除后重放事件日志即可重建；tantivy 本身不是事实来源，符合 #1 的约束。

**自写倒排**（`HashMap<词元, Vec<(doc, tf)>>` + 自算 BM25，或借用 [`bm25`](https://github.com/Michael-JB/bm25) crate 的 Scorer）：

- 优点：零重依赖、完全掌控打分与解释输出（"为何命中"）、结构化过滤可直接在内存元数据上做。
- 缺点：要么每次启动从日志重建（数十万条、上百 MB 文本时 CLI 冷启动需要秒级），要么自己设计持久化格式与增量更新，这正是 tantivy 已经做好的事。
- 估算：假设 20 万条 × 平均 100 个词元（含 bigram 字段）≈ 2000 万条倒排项，朴素 `Vec<(u32,u16)>` 约 120 MB 以上内存——对常驻 Server 可以接受，对每次调用的 CLI 不理想。

结论：规模在数千条时两者都行；按规格上限（数十万条）且有 CLI 冷启动场景，tantivy 更稳妥。把分析器作为独立模块，引擎可在 v0.1 后替换。

### 4. 无词典方案：n-gram / bigram

Lucene 的 [`CJKBigramFilter`](https://lucene.apache.org/core/9_0_0/analysis/common/org/apache/lucene/analysis/cjk/CJKBigramFilter.html) 和 Elasticsearch 的 `cjk` 分析器是这一思路的经典实现：对连续汉字输出重叠双字。优点是不依赖词典、天然覆盖新词和专名（"陈旧""倒排"这类无论词典有没有都能命中），缺点是精度低、索引更大、BM25 统计被双字噪声稀释。

tantivy 的 [`NgramTokenizer`](https://github.com/quickwit-oss/tantivy/blob/main/src/tokenizer/ngram_tokenizer.rs) 不适合直接用：它对整段文本（包括 ASCII）做 n-gram，且"position is always 0"，不能做短语查询。若用 bigram，应自写"仅对汉字段做 bigram、保留真实 position"的分词器，作为**第二字段**与 jieba 字段并查（不同 boost），而不是混在同一字段里。

### 5. SQLite FTS5

FTS5 是内嵌库（可经 `rusqlite` 的 bundled 特性静态链接），严格说不算"外部数据库"，但对中文有硬伤：默认 `unicode61` 不切中文；[`trigram` 分词器](https://sqlite.org/fts5.html#the_trigram_tokenizer)做子串匹配，但"少于 3 个 Unicode 字符的子串在全文查询中不匹配任何行"，而中文高频检索词多为 2 字（"缓存""索引"）。要用 FTS5 仍需注册自定义分词器（C API 回调接 jieba），复杂度不低于 tantivy。另外引入 SQLite 也与 #1 "存储为记忆特性定制，而非通用数据库"的立场相冲。

### 6. 代码标识符：没有现成方案覆盖全部需求

| 输入 | jieba-rs 0.11 `cut_for_search` 实测输出 | 还缺什么 |
|---|---|---|
| `append_only_log` | `append` `only` `log` `append_only_log` | — |
| `snake_case` | `snake` `case` `snake_case` | — |
| `config.yaml` | `config` `yaml` `config.yaml` | — |
| `v0.3.5` | `v0` `v0.3.5` | — |
| `getUserById` / `BeliefStore` | 原样整体，不拆 | `get` `user` `by` `id` / `belief` `store` |
| `tokio::spawn` | `tokio` `:` `:` `spawn` | 整体 `tokio::spawn`；去掉标点词元 |
| `HTTP/2` | `HTTP` `/` `2` | 整体；小写化（jieba 保留原大小写） |

（实测环境见下节。）

charabia 有 `latin-camelcase`、`latin-snakecase` 特性可参考，但为一个函数引入 charabia 不划算。建议自写一个约百行的 ASCII 标识符拆分器：先按空白和 `::` `/` `.` `-` `_` 切出"整体"，再按大小写边界拆 camelCase，所有词元小写化，整体与部分都输出。

## 本地实测（jieba-rs 0.11.0）

环境：Intel Core Ultra 7 255H，Linux x86_64，rustc/cargo 1.97.1，`--release` + `strip = true`，仅依赖 `jieba-rs = "=0.11.0"`（默认特性）。测试程序约 30 行：`Jieba::new()` 计时 → 7 条样例分别跑 `cut_for_search(hmm=true)` 与 `cut(hmm=false)` → 把样例拼接重复成 9.16 MB 文本测吞吐。

| 指标 | 结果 | 含义 |
|---|---|---|
| 内置词典词条数 | 349,045 行（`src/data/dict.txt`） | |
| 剥离符号后的可执行文件 | 3.5 MB（含 Rust 标准库与 zstd 解压） | 词典是体积的大头 |
| `Jieba::new()` | ≈86 ms | 每个进程付一次（解压词典 + 建 trie）。CLI 每次调用都会付这笔，常驻 MCP/Server 只付一次；可用惰性初始化，只在需要分词时加载 |
| `cut_for_search` 吞吐 | ≈90 MiB/s（9.16 MB → 224 万词元，98 ms，单线程） | 样例重复导致缓存友好，偏乐观；即便打一个数量级折扣，数十万条记忆的全量重建分词也在秒级 |

切分质量样例（`cut_for_search`）：

- 常规技术中文切得好：“缓存/失效/导致/记忆/检索/结果/陈旧”，“修复/了/…/连接/连接池/…/死锁/问题”。
- **新词/领域词切碎**：“智能体”→`智能` `体`；“提示词”→`提示` `词`；“大语言模型”→`大` `语言` `模型`。这对检索影响有限（查询端同样切碎，仍能命中），但会损失精度——用户词典（`add_word`）或 bigram 字段可补。
- **误切**：“缺字段”→`缺字` `段`，查“字段”将无法从 jieba 字段命中——这正是需要 bigram 兜底字段的直接证据（bigram 会产生 `字段`）。
- 标点会成为词元（`，` `:` `/`），分析器需过滤。

## 推荐方案

**分析器（v0.1 必须定）**

1. 输入先做 NFKC 归一化 + 小写化（全角 → 半角，便于中英混排）。
2. 按字符类切段：汉字段 → `jieba-rs` `cut_for_search(hmm=true)`；ASCII 段 → 自写标识符拆分（整体 + 部分）。
3. 第二字段：汉字段输出重叠 bigram（单字段落输出 unigram），用于新词兜底召回；查询时两个字段并查，jieba 字段权重更高。
4. 用户词典（项目术语、人名、库名）作为配置随事件日志持久化，加载到 `Jieba::add_word`。
5. 分析器指纹 = jieba-rs 版本 + 词典哈希 + 用户词典哈希 + 自写规则版本；写入索引元数据，不一致即全量重建。

**索引引擎**

- 推荐 `tantivy`：BM25、短语、快速字段过滤、持久化 mmap、CLI 冷启动快；用自写分析器实现 `tantivy::tokenizer::Tokenizer`。写入路径在 almanac 内部串行化以适配单写者锁。
- 备选：若实测 tantivy 依赖/二进制体积不可接受，且规模保持在数万条以内，可改为自写内存倒排 + 启动时从日志重建（或从快照加载）。分析器不变。

## 未验证 / 待后续

- 未实测 tantivy 依赖树编译时间和二进制增量。
- 未找到 jieba-rs 在技术文本（混排代码）上的分词准确率公开评测；仅做了少量样例实测。
- 未比较 HMM 开/关对召回的影响（HMM 可能把未登录词切得不稳定，需要基于真实记忆语料评估）。
- 繁体中文：jieba 词典以简体为主；charabia 的 kvariant 归一化可作参考，v0.1 未覆盖。
