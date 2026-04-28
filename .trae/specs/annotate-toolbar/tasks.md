# Tasks

- [x] Task 1: 重构数据结构，定义标注元素模型
  - 定义 `Annotation` 枚举，包含 Rectangle、Ellipse、Arrow、FreeDraw、Text、Mosaic 六种变体，每种变体携带其位置信息和样式（颜色、粗细、文字大小等）
  - 定义 `Tool` 枚举，表示当前选中的标注工具（含 None 表示选区间拖拽）
  - 定义 `StrokeWidth` 枚举（Thin=2px, Medium=4px, Thick=6px）和 `LineColor` 枚举（5 种预设色）
  - 定义 `FontSize` 枚举（Small=16px, Medium=24px, Large=36px）
  - 将 `AppState` 扩展为 `Selecting` / `Adjusting` / `Canvas` 三种状态
  - 在 `ScreenshotApp` 中添加 `annotations: Vec<Annotation>`、`current_tool: Tool`、`stroke_width: StrokeWidth`、`line_color: LineColor`、`font_size: FontSize`、`text_input_buffer: String`、`text_cursor_pos: Option<Pos2>` 等字段

- [x] Task 2: 实现选区拖拽调整（Adjusting 模式）
  - 选区确认后，选区四角显示 8×8 拖拽手柄
  - 拖拽边角手柄时实时调整选区大小
  - 选区调整后重新计算选区相对于屏幕的位置

- [x] Task 3: 实现工具栏 UI 渲染（两行布局）
  - 第一行（36px）：标注工具按钮（矩形□、圆形○、箭头↗、画笔✎、文字T、马赛克▩），中间竖线分隔，右侧操作按钮（撤销↩、关闭✕、确认✓），当前选中工具高亮
  - 第二行（30px）：样式配置区 —— 当选中画笔类工具（矩形/圆形/箭头/画笔）时显示粗细选择（细=, 中=, 粗≡）和颜色色块（红●黄●绿●蓝●白●）；当选中文字工具时显示字号选择（小A 中A 大A）和颜色色块；当选中马赛克工具时第二行不显示；当前选中的粗细/字号/颜色高亮；按钮图标使用 Unicode 字符或简单文本绘制

- [x] Task 4: 实现矩形标注工具
  - 选择矩形工具后，在画布上拖拽绘制矩形
  - 矩形使用当前的 `line_color` 和 `stroke_width` 渲染边框，半透明填充
  - 拖拽结束时将矩形注释存入 `annotations`

- [x] Task 5: 实现圆形/椭圆标注工具
  - 选择圆形工具后，在画布上拖拽绘制椭圆
  - 椭圆使用当前的 `line_color` 和 `stroke_width` 渲染边框
  - 拖拽结束时将椭圆注释存入 `annotations`

- [x] Task 6: 实现箭头标注工具
  - 选择箭头工具后，在画布上拖拽绘制箭头
  - 从按下点到释放点绘制线段，使用当前的 `line_color` 和 `stroke_width`
  - 终点显示箭头尖端（由两条短线段构成，夹角约 40°，长度约 14px）
  - 拖拽结束时将箭头注释存入 `annotations`

- [x] Task 7: 实现画笔标注工具
  - 选择画笔工具后，按住鼠标在画布上拖动绘制曲线
  - 沿鼠标移动轨迹使用当前的 `line_color` 和 `stroke_width` 渲染线段
  - 收集鼠标移动轨迹点构成折线，松开鼠标时将折线注释存入 `annotations`

- [x] Task 8: 实现文字标注工具（支持中文输入）
  - 选择文字工具后，点击画布位置将坐标存入 `text_cursor_pos`，激活文字输入
  - 使用 `egui` 的 `ctx.input(|i| i.events)` 捕获键盘事件处理 IME 输入（支持中文输入法）
  - 将输入字符追加到 `text_input_buffer`，实时渲染在画布上
  - 文字使用当前的 `font_size` 和 `line_color` 渲染
  - 按 Enter 确认文字，将 Text Annotation（含内容、位置、字号、颜色）存入 `annotations`
  - 按 Esc 取消当前文字输入

- [x] Task 9: 实现马赛克标注工具
  - 选择马赛克工具后，在画布上拖拽绘制矩形区域
  - 区域内渲染像素化效果（马赛克块大小约 10×10 像素，取块内像素平均值）
  - 拖拽结束时将马赛克注释存入 `annotations`

- [x] Task 10: 实现撤销功能
  - 点击撤销按钮（或 Ctrl+Z）弹出 `annotations` 最后一个元素
  - 添加 `arboard` 依赖到 Cargo.toml

- [x] Task 11: 实现标注合成、保存到文件和剪贴板
  - 实现 `compose()` 方法：将截图选区 + 所有标注合成为一张 RGBA 图片
  - 下载按钮：调用 `compose()` 生成 PNG，以 `screenshot_时间戳.png` 保存到文件
  - 对号按钮：调用 `compose()` → 转为 `arboard::ImageData` 写入系统剪贴板，退出
  - X 按钮和 Esc 键直接退出不保存

# Task Dependencies
- Task 1 是所有后续任务的前置依赖
- Task 2、Task 3 依赖 Task 1
- Task 4-9 依赖 Task 1、Task 3
- Task 10 依赖 Task 4-9 中至少一个完成
- Task 11 依赖 Task 1、Task 3，可与 Task 4-10 并行开发，最终集成
