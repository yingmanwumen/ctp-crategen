# ctp-crategen

`ctp-crategen` 根据 CTP SDK 目录生成 Rust crate。生成结果包含 bindgen 生成的 FFI 类型、C++ bridge、请求 builder、Trader/MD SPI trait，以及基础 examples。

本项目采用 `MIT OR Apache-2.0` 双许可证。

> 当前版本仅支持 **Linux**。Windows、macOS 和其他平台尚未支持。

## 工作原理

生成流程如下：

1. 读取 SDK headers、动态库和可选的 `error.xml`。
2. 解析 Trader/MD API 与 SPI 声明。
3. 生成 C++ bridge，将 CTP C++ API 暴露给 Rust。
4. 使用 bindgen 生成 `src/sys.rs`，保留 CTP 原始 ABI 类型。
5. 生成 Rust API wrapper、请求 builder 和 SPI callback trampoline。
6. 复制 SDK 文件到生成 crate 的 `vendor/` 目录。

如果某个动态库内部声明了与文件名不同的 ELF `SONAME`，生成阶段会在
`vendor/` 中自动创建 SONAME 到实际文件名的软链接。例如：

```text
thosttraderapi_se_6.7.2.so -> thosttraderapi_se.so
```

因此可以在生成后直接替换 `vendor/thosttraderapi_se.so`，不需要手动修改
SONAME 或创建软链接。替换 library 的其他依赖也需要放入 `vendor/`，例如
`librohonbase.so` 和 `libLinuxDataCollect.so`。

构建生成 crate 时，`build.rs` 会把 `vendor/` 中的动态库 staging 到 bridge
所在目录，并将 bridge 的运行时搜索路径设置为 `$ORIGIN`。因此 bridge 不会
记录本机 SDK 目录的绝对路径；构建目录移动后，bridge 仍会从自身目录查找
CTP library。

上述动态库处理依赖 Linux ELF 的 SONAME 和 Unix symbolic link 机制，不能直接
用于 Windows `.dll` 或 macOS `.dylib`。

生成 crate 的高级 API 不复制另一套 CTP struct。请求和回调仍然使用 bindgen struct，以保持 ABI、字段完整性和不同 SDK 版本的兼容性；builder 和 helper 负责减少常见 FFI 操作。

## 构建

要求：

- Linux x86_64 环境
- Rust toolchain
- C++ compiler
- libclang，以及 bindgen 所需的 clang 环境
- Linux CTP SDK 目录及其 `.so` 动态库

生成一个 crate：

```bash
cargo run -- generate \
  /path/to/ctp-sdk \
  --output-dir /tmp/ctp-generated \
  --crate-name ctp_generated
```

如果 SDK 中包含 `error.xml`，默认会生成 `BusinessErrorCode`。可以关闭错误码生成：

```bash
cargo run -- generate \
  /path/to/ctp-sdk \
  --output-dir /tmp/ctp-generated \
  --error-xml=false
```

进入生成 crate 后构建：

```bash
cd /tmp/ctp-generated
cargo check
```

## 生成 crate 的使用

### 查看 SDK version

生成 crate 自带 `version` example：

```bash
cargo run --example version
```

它会打印 Trader API 和 MD API version。

### API 和 SPI

API 通过 builder 创建，SPI handler 通过 `.spi(...)` 注册：

```rust
let trader = TraderApi::builder()
    .flow_path("./flow/trader")
    .register_front("tcp://127.0.0.1:41205")
    .spi(MyTraderHandler)
    .build()?;
```

请求使用生成的 builder：

```rust
let mut request = ReqUserLogin::builder()
    .broker_id("9999")
    .user_id("user")
    .password("password")
    .build()?;

trader.req_user_login(&mut request, request_id)?;
```

请求方法最终接收 bindgen struct。这是有意设计：`sys` 层保留完整 FFI 类型，高级 builder 只负责初始化和转换常用字段。

`MdApi` 和 `TraderApi` 实现 `Send + Sync`，可以通过 `Arc` 在多个 application thread
之间共享并调用 `&self` 请求方法，无需为 API 实例额外添加 `Mutex`。这依赖 CTP SDK
对同一实例并发请求的线程安全保证。需要 `&mut self` 的方法仍要求独占访问。
SPI handler 同样要求 `Send + Sync`；handler 的共享可变状态和业务请求顺序由应用自行管理。

### 组合开平和投机套保

CTP 的 `CombOffsetFlag` 和 `CombHedgeFlag` 在 FFI 中是长度为 5 的字符数组。高级 builder 不要求调用者手动填写数组：

```rust
let request = InputOrder::builder()
    .direction(sys::THOST_FTDC_D_Buy)
    .comb_offset_flag(sys::THOST_FTDC_OF_Open)
    .build()?;
```

builder 会设置数组第一个元素并清零其余元素。`CombHedgeFlag` 默认是 `Speculation`，也可以显式调用 `.comb_hedge_flag(...)` 覆盖。

## 字符串和编码

### Rust 输入

Rust 的 `String` 和 `&str` 始终是 UTF-8。

CTP 的文本字段使用 GBK/GB2312 兼容编码。生成的文本 setter 会在写入 CTP 的 `char[]` 前执行：

```text
Rust UTF-8 string -> GBK bytes -> CTP char buffer
```

ASCII 字符、数字和常见符号在 UTF-8 与 GBK 中使用相同字节，因此 BrokerID、UserID、InstrumentID、ExchangeID 等字段不受影响。

中文请求字段，例如客户姓名、登录备注、订单备注或银期附加信息，会使用 GBK 字节写入。字符串包含 NUL、超过目标 buffer 容量，或包含无法转换为 GBK 的字符时，`build()` 返回错误。

二进制字段不作为文本处理，使用 `&[u8]` setter，并按照 SDK 的长度字段自动设置长度。

### 回调输出

CTP 回调中的中文字段通常是 GBK bytes。生成 crate 提供：

```rust
decode_ctp_text(&field.SomeTextField)
```

解码顺序是严格 UTF-8、严格 GBK，最后才使用 replacement characters 作为异常数据的 fallback。

常见错误字段提供快捷方法：

```rust
info.error_msg()
order.status_msg()
order_action.status_msg()
```

回调对象本身仍然是 bindgen struct；调用者可以通过 `sys` 字段访问 CTP 的完整数据，也可以使用上述字符串 helper 读取文本。

## FFI 边界

`sys` 模块是底层 FFI 层，保留：

- bindgen struct
- CTP typedef
- CTP enum constants
- C ABI callback 参数

高级层提供 builder、生命周期管理、SPI trait 和字符串/数组 helper，但不会隐藏所有原始 CTP 字段。这样可以避免维护第二套与 SDK 同步的 Rust struct，并保留 SDK 新字段的可访问性。

## Examples

生成 crate 包含：

- `examples/version.rs`：打印 Trader/MD API version。
- `examples/basic.rs`：展示 API/SPI 创建、认证、登录、行情订阅、可选报单、撤单和回报处理。

`basic` example 默认只监听，不发送订单。只有设置 `CTP_ENABLE_ORDERS=1` 时才启用订单示例。
