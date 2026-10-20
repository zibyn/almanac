# almanac

为多种智能体提供外部长期记忆的基础设施。信念会变，证据永存。

## Language

**Evidence（证据）**:
一条不可变的原始记录（一段对话、一份文件、一次工具输出），连同它的来源信息。只追加，从不修改。
_Avoid_: 原始数据、日志、raw memory

**Belief（信念）**:
从证据中提炼出的一条论断，必须引用支撑它的证据；可以被更新、取代或遗忘。
_Avoid_: memory（单独使用时有歧义）、fact、知识

**Store（记忆库）**:
属于一个人的全部证据与信念，被这个人的多台设备、多个智能体共享。
_Avoid_: 数据库、仓库、vault

**Scope（作用域）**:
一条信念适用的范围：全局（关于这个人本身）或项目（关于某个代码库/工作区）。
_Avoid_: namespace、context

**Writer（写入方）**:
执行某次写入的智能体身份（如 Codex、Claude Code、Pi），是来源信息的一部分。
_Avoid_: client、user、author

**Provenance（来源）**:
证据从何而来的信息：来源类型、位置、时间、写入方，以及可用于再次核验的指纹。
_Avoid_: source（单独使用时有歧义）、metadata
