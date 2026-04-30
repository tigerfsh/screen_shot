# 开发日志：文字标注中文输入与显示

## 背景

截图工具的文字标注（Text tool）初始只支持英文输入。用户使用搜狗输入法（fcitx5）在 Linux X11 下输入中文时，拼音字母逐个透传，无法正常输入中文字符。

---

## 第一阶段：IME 事件处理

### 问题现象
输入中文时，只看到拼音字母（n-i-h-a-o）逐个出现，没有中文候选。

### 排查结果
`handle_text_input` 只监听 `Event::Text`，没有处理 `Event::Ime`。`ImeEvent::Preedit`（候选文本）和 `ImeEvent::Commit`（提交文本）完全未处理。

### 修复
添加 `ImeEvent::Preedit` / `ImeEvent::Commit` / `ImeEvent::Disabled` 处理逻辑，新增 `ime_preedit` 字段存储编辑中的中间态文本。同时调用 `ctx.send_viewport_cmd(IMEAllowed(true))` 和 `IMERect()` 设置 IME 光标位置。

**文件**: `src/main.rs`, `apply_text_events()` 函数

---

## 第二阶段：事件处理顺序 Bug

### 问题现象
中文仍然无法输入。添加诊断日志后，发现虽然 `Event::Text("你好")` 确实到达，但 `finish_text_input()` 保存的总是空内容。

### 排查结果
日志显示 `Event::Text("你好")` 和 `Enter` 键在**同一帧**到达。但 `update()` 中先执行了 `key_pressed(Enter)` → `finish_text_input()`（保存空 buffer），**然后**才执行 `handle_text_input()` 读入中文。

### 修复
将 `self.handle_text_input(ctx)` 从 `update()` 中 `key_pressed(Enter)` 检查之后移动到**最前面**（所有按键处理之前）。同时添加 `IMEPurpose::Normal`。

**关键代码**:
```rust
// update() 中的顺序：
self.ensure_ime(ctx);          // ① 先启用 IME
self.handle_text_input(ctx);   // ② 先收集所有文本事件
// ... 然后才处理 key_pressed(Enter) 等按键
```

---

## 第三阶段：egui-winit 主动丢弃 Linux IME 事件

### 问题现象
即使代码逻辑正确，`ImeEvent::Commit` 仍然不到达。日志显示拼音字母仍以 `Event::Text("n")` 逐个到达，但**没有任何 `Event::Ime` 事件**。

### 排查结果
阅读 egui-winit 0.30 源码 `src/lib.rs`，发现在 Linux 上**明确跳过所有 IME 事件**：

```rust
// egui-winit 0.30.0/src/lib.rs line 336
WindowEvent::Ime(ime) => {
    if cfg!(target_os = "linux") {
        // We ignore IME events on linux, because of issue #5008
    } else {
        // ... Preedit / Commit processing ...
    }
}
```

winit 的 X11 后端完整实现了 XIM/XIC，正确产生 `Ime::Preedit("你好")` / `Ime::Commit("你好")` 事件，但 egui-winit 在 Linux 上直接丢弃。

### 修复
使用 Cargo `[patch.crates-io]` 机制，不修改原始 registry，而是：

1. **`vendor/egui-winit/`** — 复制 egui-winit 0.30.0 到本地，删除 `cfg!(target_os = "linux")` 阻断
2. **`Cargo.toml`** — 添加:
   ```toml
   [patch.crates-io]
   egui-winit = { path = "vendor/egui-winit" }
   ```

**文件**: `vendor/egui-winit/src/lib.rs`, `Cargo.toml`

---

## 第四阶段：中文字形渲染

### 问题现象
中文输入成功并被保存到注解，但保存后的截图里中文显示为方块或空白，文字预览（实时显示）也看不到中文。

### 排查结果
1. **图片合成渲染**（`draw_text_on_image`）使用 `ab_glyph` + `load_font()` 加载字体。原来字体路径优先加载 DejaVuSans（不含中文），跳过了 NotoSansCJK。
2. **屏幕实时预览**（`render_annotations`）使用 `egui::FontId::proportional()`，而 egui 默认字体不含 CJK 字形。

### 修复

**图片合成**：重新排列 `load_font()` 字体路径，优先加载 CJK 字体：
- `/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc`
- `/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc`
- ... 其他 CJK 字体
- DejaVuSans 放到最后作为回退

**屏幕预览**：在 `update()` 第一帧注册 CJK 字体到 egui：
```rust
if !self.font_registered {
    if let Some(font_data) = Self::load_font() {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("cjk".into(), Arc::new(egui::FontData::from_owned(font_data)));
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "cjk".into());
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "cjk".into());
        ctx.set_fonts(fonts);
    }
    self.font_registered = true;
}
```

**文件**: `src/main.rs`, `load_font()`, `update()`

---

## 最终验证

运行 `cargo run`，用搜狗输入法输入中文：

```
[Font] Loaded: /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc
[IME] First time sending IMEAllowed(true) + IMEPurpose
[TextInput] Event: Ime(Enabled)
[TextInput] Event: Ime(Commit("你好"))
[Update] Enter pressed, calling finish_text_input, buf: "你好"
[TextInput] finish_text_input: saving text "你好"
```

✅ 中文输入正确
✅ 实时预览显示中文
✅ 保存截图中文正确渲染
✅ 复制到剪贴板中文正常

---

## 附加：测试用例

新增 11 个单元测试覆盖文本输入逻辑，包括：

| 测试 | 场景 |
|------|------|
| `chinese_via_event_text` | 中文通过 `Event::Text` 到达 |
| `chinese_via_ime_commit` | 中文通过 `ImeEvent::Commit` 到达 |
| `chinese_multi_word_ime` | 多词连续输入 |
| `chinese_mixed_english` | 中英文混合输入 |
| `chinese_punctuation` | 中文标点符号 |
| `chinese_text_and_enter_same_frame` | Commit 与 Enter 同帧到达的临界 Bug |
| `ime_preedit_canceled_with_backspace` | Backspace 取消预编辑 |
| `ime_commit_then_preedit_does_not_overflow` | 提交后新预编辑正确 |
| `empty_commit_is_handled` | 空提交正确处理 |

运行: `cargo test` — 全部通过。
