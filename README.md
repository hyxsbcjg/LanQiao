# 蓝桥杯单片机在线仿真器

[打开仿真器](https://lanqiao.thecode.blog/) · [下载按键示例 HEX](https://github.com/hyxsbcjg/LanQiao/raw/refs/heads/main/engine/samples/key_seg/prj/Objects/key_seg.hex)

把 Keil 编译的 HEX 拖进网页，就能在浏览器里操作虚拟实验板。固件只在本地运行。试用按键示例时，保持 **KBD · 矩阵** 模式，按住 **S4**，数码管显示 **1**；松开恢复 **0**。

## 本地运行

需要 Node.js 22.12 或更新版本。仓库自带 WASM 和板图，可直接启动：

```sh
npm ci
npm run dev
```

打开 <http://127.0.0.1:5179/>。`npm run build` 检查 TypeScript 并将静态站点生成到 `dist/`。

## 重建仿真引擎

只有修改 Rust 引擎才需要这一步。在 Linux 或 WSL 的仓库根目录运行：

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked
bash engine/crates/web/build.sh "$PWD/src/generated/engine"
cargo test --locked --workspace --manifest-path engine/Cargo.toml
```

## 许可与来源

本项目使用 [AGPL-3.0-or-later](LICENSE)。`engine/` 基于 **azazo1**（`azazo1@qq.com`）及贡献者的 [lanqiao-mcu-judge，提交 f814150](https://github.com/azazo1/lanqiao-mcu-judge/tree/f814150d173d3bbb54893f58be814e55b8d59fb8)。本版本增加了浏览器 WASM 接口、V2 板级交互、显示亮度积分和可选主频下的串口时序，保留 Web 仿真使用的引擎核心。引擎测试固件的源程序见[上游示例](https://github.com/azazo1/lanqiao-mcu-judge/tree/f814150d173d3bbb54893f58be814e55b8d59fb8/samples)。其中 [i8051](https://github.com/mmastrac/i8051) 由 Matt Mastracci 开发，按 MIT 或 Apache-2.0 授权。
