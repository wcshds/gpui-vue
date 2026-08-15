//! Typed localized interface copy for the KAGE Editor example package.
//!
//! The catalogue is deliberately implemented as exhaustive matches rather
//! than a runtime map. This keeps lookups allocation-free and makes a missing
//! translation a compile-time error whenever a new [`UiText`] key is added.

use super::model::UiLanguage;

/// A localized piece of visible KAGE Editor interface copy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiText {
    /// Product name shown in the title bar.
    AppName,
    /// Default document title and canvas dimensions.
    UntitledDocument,
    /// Status shown when the KAGE engine is ready.
    ReadyEngineConnected,
    /// Status shown when the document has no uncommitted changes.
    Clean,
    /// Action that closes and accepts a sheet.
    Done,
    /// Import action.
    Import,
    /// KAGE source export action.
    ExportKage,
    /// Action that copies the complete exported source.
    CopyAll,
    /// Confirmation shown after copying exported source.
    Copied,
    /// Explanation of filtering applied to the exported source.
    FilteredKageHint,
    /// Cut action.
    Cut,
    /// Copy action.
    Copy,
    /// Paste action.
    Paste,
    /// Delete action.
    Delete,
    /// Selection tool label.
    SelectionTool,
    /// Intelligent freehand tool label.
    FreehandTool,
    /// Line insertion action.
    AddLine,
    /// Bézier curve insertion action.
    AddCurve,
    /// Inspector sidebar tab.
    Inspector,
    /// Components sidebar tab.
    Components,
    /// Layers sidebar tab.
    Layers,
    /// Selection inspector section.
    Selection,
    /// Empty selection state.
    NoSelection,
    /// Stroke-type inspector section.
    StrokeType,
    /// Stroke-style inspector section.
    Style,
    /// Stroke type field.
    Type,
    /// Stroke head field.
    Head,
    /// Stroke tail field.
    Tail,
    /// Selection bounds field.
    Bounds,
    /// Control-points field.
    Points,
    /// Transform inspector section.
    Transform,
    /// Metadata inspector section.
    Metadata,
    /// Engine-transform inspector section.
    EngineTransform,
    /// Document-health inspector section.
    DocumentHealth,
    /// Healthy-document state.
    NoStructuralIssues,
    /// Component inspector section.
    Component,
    /// Component-name field.
    Name,
    /// Component-stretch field.
    Stretch,
    /// Component decomposition action.
    DecomposeToStrokes,
    /// Component-search placeholder.
    SearchComponentsPlaceholder,
    /// Empty component-search state.
    NoComponentMatches,
    /// Layer paint-order section.
    PaintOrder,
    /// Empty layer-list state.
    NoLayers,
    /// Move a layer one step toward the front.
    BringForward,
    /// Move a layer one step toward the back.
    SendBackward,
    /// Preferences appearance section.
    DocumentAppearance,
    /// Typeface preference.
    Typeface,
    /// Mincho typeface option.
    Mincho,
    /// Gothic typeface option.
    Gothic,
    /// Skeleton-only typeface option.
    Skeleton,
    /// Curve-aware Mincho outline preference.
    SmoothStrokes,
    /// Centerline preference.
    Centerlines,
    /// Hidden-centerlines option.
    CenterlineNone,
    /// Selected-centerlines option.
    CenterlineSelection,
    /// Always-visible centerlines option.
    CenterlineAlways,
    /// Mask preference.
    Mask,
    /// Disabled-mask option.
    MaskNone,
    /// Circular-mask option.
    MaskCircle,
    /// Rounded-square-mask option.
    MaskRoundedSquare,
    /// Square-mask option.
    MaskSquare,
    /// Diamond-mask option.
    MaskDiamond,
    /// Interface-language preference.
    Language,
    /// English language name.
    English,
    /// Japanese language name.
    Japanese,
    /// Korean language name.
    Korean,
    /// Simplified Chinese language name.
    SimplifiedChinese,
    /// Traditional Chinese language name.
    TraditionalChinese,
    /// Grid preference and toolbar action.
    Grid,
    /// Horizontal grid-origin field.
    GridOriginX,
    /// Vertical grid-origin field.
    GridOriginY,
    /// Horizontal grid-spacing field.
    GridSpacingX,
    /// Vertical grid-spacing field.
    GridSpacingY,
    /// Grid-snapping preference.
    SnapToGrid,
    /// Enabled state.
    On,
    /// Disabled state.
    Off,
    /// Technical label for one serialized KAGE record.
    KageRecord,
    /// Hint that transforms preserve connected points.
    ConnectedGeometry,
    /// Transform-operation field.
    Operation,
    /// Document state requiring attention.
    Review,
    /// Neutral component-stretch option.
    Neutral,
    /// Horizontal flip action.
    FlipHorizontal,
    /// Vertical flip action.
    FlipVertical,
    /// Counter-clockwise rotation action.
    RotateLeft,
    /// Clockwise rotation action.
    RotateRight,
    /// Component-search result-count label.
    Matches,
    /// Component-search descendant-discovery hint.
    FindDescendants,
    /// Grid snapping scope hint.
    SnapPointerHint,
    /// Online engine status.
    EngineOnline,
    /// Bend stroke-kind name.
    StrokeBend,
    /// Corner stroke-kind name.
    StrokeCorner,
    /// Bézier stroke-kind name.
    StrokeBezier,
    /// Sweep stroke-kind name.
    StrokeSweep,
}

impl UiText {
    /// Every key in stable visual-group order.
    #[cfg(test)]
    pub const ALL: [Self; 92] = [
        Self::AppName,
        Self::UntitledDocument,
        Self::ReadyEngineConnected,
        Self::Clean,
        Self::Done,
        Self::Import,
        Self::ExportKage,
        Self::CopyAll,
        Self::Copied,
        Self::FilteredKageHint,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::Delete,
        Self::SelectionTool,
        Self::FreehandTool,
        Self::AddLine,
        Self::AddCurve,
        Self::Inspector,
        Self::Components,
        Self::Layers,
        Self::Selection,
        Self::NoSelection,
        Self::StrokeType,
        Self::Style,
        Self::Type,
        Self::Head,
        Self::Tail,
        Self::Bounds,
        Self::Points,
        Self::Transform,
        Self::Metadata,
        Self::EngineTransform,
        Self::DocumentHealth,
        Self::NoStructuralIssues,
        Self::Component,
        Self::Name,
        Self::Stretch,
        Self::DecomposeToStrokes,
        Self::SearchComponentsPlaceholder,
        Self::NoComponentMatches,
        Self::PaintOrder,
        Self::NoLayers,
        Self::BringForward,
        Self::SendBackward,
        Self::DocumentAppearance,
        Self::Typeface,
        Self::Mincho,
        Self::Gothic,
        Self::Skeleton,
        Self::SmoothStrokes,
        Self::Centerlines,
        Self::CenterlineNone,
        Self::CenterlineSelection,
        Self::CenterlineAlways,
        Self::Mask,
        Self::MaskNone,
        Self::MaskCircle,
        Self::MaskRoundedSquare,
        Self::MaskSquare,
        Self::MaskDiamond,
        Self::Language,
        Self::English,
        Self::Japanese,
        Self::Korean,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Grid,
        Self::GridOriginX,
        Self::GridOriginY,
        Self::GridSpacingX,
        Self::GridSpacingY,
        Self::SnapToGrid,
        Self::On,
        Self::Off,
        Self::KageRecord,
        Self::ConnectedGeometry,
        Self::Operation,
        Self::Review,
        Self::Neutral,
        Self::FlipHorizontal,
        Self::FlipVertical,
        Self::RotateLeft,
        Self::RotateRight,
        Self::Matches,
        Self::FindDescendants,
        Self::SnapPointerHint,
        Self::EngineOnline,
        Self::StrokeBend,
        Self::StrokeCorner,
        Self::StrokeBezier,
        Self::StrokeSweep,
    ];
}

/// Returns localized interface copy without allocation or runtime lookup state.
#[must_use]
pub const fn text(language: UiLanguage, key: UiText) -> &'static str {
    match language {
        UiLanguage::English => match key {
            UiText::AppName => "KAGE Editor",
            UiText::UntitledDocument => "UNTITLED · 200 × 200",
            UiText::ReadyEngineConnected => "Ready · KAGE engine connected",
            UiText::Clean => "Clean",
            UiText::Done => "Done",
            UiText::Import => "Import",
            UiText::ExportKage => "Export KAGE",
            UiText::CopyAll => "Copy all",
            UiText::Copied => "Copied",
            UiText::FilteredKageHint => "Only exportable KAGE records are included.",
            UiText::Cut => "Cut",
            UiText::Copy => "Copy",
            UiText::Paste => "Paste",
            UiText::Delete => "Delete",
            UiText::SelectionTool => "Select",
            UiText::FreehandTool => "Ink",
            UiText::AddLine => "Line",
            UiText::AddCurve => "Curve",
            UiText::Inspector => "Inspector",
            UiText::Components => "Components",
            UiText::Layers => "Layers",
            UiText::Selection | UiText::CenterlineSelection => "Selection",
            UiText::NoSelection => "Nothing selected",
            UiText::StrokeType => "Stroke type",
            UiText::Style => "Style",
            UiText::Type => "Type",
            UiText::Head => "Head",
            UiText::Tail => "Tail",
            UiText::Bounds => "Bounds",
            UiText::Points => "Points",
            UiText::Transform => "Transform",
            UiText::Metadata => "Metadata",
            UiText::EngineTransform => "Engine transform",
            UiText::DocumentHealth => "Document health",
            UiText::NoStructuralIssues => "No structural issues",
            UiText::Component => "Component",
            UiText::Name => "Name",
            UiText::Stretch => "Stretch",
            UiText::DecomposeToStrokes => "Decompose to strokes",
            UiText::SearchComponentsPlaceholder => "Search radicals, names, or Unicode…",
            UiText::NoComponentMatches => "No matching components",
            UiText::PaintOrder => "KAGE record order · first to last",
            UiText::NoLayers => "No layers in this glyph",
            UiText::BringForward => "Bring forward",
            UiText::SendBackward => "Send backward",
            UiText::DocumentAppearance => "Document appearance",
            UiText::Typeface => "Typeface",
            UiText::Mincho => "Mincho",
            UiText::Gothic => "Gothic",
            UiText::Skeleton => "Skeleton",
            UiText::SmoothStrokes => "Smooth strokes · Mincho only",
            UiText::Centerlines => "Centerlines",
            UiText::CenterlineNone | UiText::MaskNone => "None",
            UiText::CenterlineAlways => "Always",
            UiText::Mask => "Mask",
            UiText::MaskCircle => "Circle",
            UiText::MaskRoundedSquare => "Rounded square",
            UiText::MaskSquare => "Square",
            UiText::MaskDiamond => "Diamond",
            UiText::Language => "Language",
            UiText::English => "English",
            UiText::Japanese => "Japanese",
            UiText::Korean => "Korean",
            UiText::SimplifiedChinese => "Simplified Chinese",
            UiText::TraditionalChinese => "Traditional Chinese",
            UiText::Grid => "Grid",
            UiText::GridOriginX => "X origin",
            UiText::GridOriginY => "Y origin",
            UiText::GridSpacingX => "X spacing",
            UiText::GridSpacingY => "Y spacing",
            UiText::SnapToGrid => "Snap to grid",
            UiText::On => "On",
            UiText::Off => "Off",
            UiText::KageRecord => "KAGE record",
            UiText::ConnectedGeometry => "Connected geometry",
            UiText::Operation => "Operation",
            UiText::Review => "Review",
            UiText::Neutral => "Neutral",
            UiText::FlipHorizontal => "Flip left-right",
            UiText::FlipVertical => "Flip top-bottom",
            UiText::RotateLeft => "Rotate left",
            UiText::RotateRight => "Rotate right",
            UiText::Matches => "matches",
            UiText::FindDescendants => "⇧ click · find descendants",
            UiText::SnapPointerHint => "Applies to pointer-driven edits",
            UiText::EngineOnline => "Engine online",
            UiText::StrokeBend => "Bend",
            UiText::StrokeCorner => "Corner",
            UiText::StrokeBezier => "Bézier",
            UiText::StrokeSweep => "Sweep",
        },
        UiLanguage::Japanese => match key {
            UiText::AppName => "KAGE Editor",
            UiText::UntitledDocument => "名称未設定 · 200 × 200",
            UiText::ReadyEngineConnected => "準備完了 · KAGE エンジン接続済み",
            UiText::Clean => "変更なし",
            UiText::Done => "完了",
            UiText::Import => "読み込む",
            UiText::ExportKage => "KAGE を出力",
            UiText::CopyAll => "すべてコピー",
            UiText::Copied => "コピー済み",
            UiText::FilteredKageHint => "出力可能な KAGE レコードのみが含まれます。",
            UiText::Cut => "切り取り",
            UiText::Copy => "コピー",
            UiText::Paste => "ペースト",
            UiText::Delete => "削除",
            UiText::SelectionTool | UiText::Selection => "選択",
            UiText::FreehandTool => "インク",
            UiText::AddLine => "直線",
            UiText::AddCurve => "曲線",
            UiText::Inspector => "インスペクタ",
            UiText::Components | UiText::Component => "部品",
            UiText::Layers => "レイヤー",
            UiText::NoSelection => "選択されていません",
            UiText::StrokeType => "画タイプ",
            UiText::Style => "スタイル",
            UiText::Type => "タイプ",
            UiText::Head => "始端",
            UiText::Tail => "終端",
            UiText::Bounds => "境界",
            UiText::Points => "制御点",
            UiText::Transform => "変形",
            UiText::Metadata => "メタデータ",
            UiText::EngineTransform => "エンジン変形",
            UiText::DocumentHealth => "文書の状態",
            UiText::NoStructuralIssues => "構造上の問題はありません",
            UiText::Name => "名前",
            UiText::Stretch => "伸縮",
            UiText::DecomposeToStrokes => "筆画に分解",
            UiText::SearchComponentsPlaceholder => "部首、名前、Unicode を検索…",
            UiText::NoComponentMatches => "一致する部品はありません",
            UiText::PaintOrder => "KAGE レコード順 · 先から後",
            UiText::NoLayers => "この字形にはレイヤーがありません",
            UiText::BringForward => "前面へ",
            UiText::SendBackward => "背面へ",
            UiText::DocumentAppearance => "文書の外観",
            UiText::Typeface => "書体",
            UiText::Mincho => "明朝体",
            UiText::Gothic => "ゴシック体",
            UiText::Skeleton => "骨格のみ",
            UiText::SmoothStrokes => "滑らかな筆画 · 明朝体のみ",
            UiText::Centerlines => "中心線",
            UiText::CenterlineNone | UiText::MaskNone => "なし",
            UiText::CenterlineSelection => "選択中のみ",
            UiText::CenterlineAlways => "常に表示",
            UiText::Mask => "マスク",
            UiText::MaskCircle => "円",
            UiText::MaskRoundedSquare => "角丸四角形",
            UiText::MaskSquare => "四角形",
            UiText::MaskDiamond => "ひし形",
            UiText::Language => "言語",
            UiText::English => "英語",
            UiText::Japanese => "日本語",
            UiText::Korean => "韓国語",
            UiText::SimplifiedChinese => "簡体字中国語",
            UiText::TraditionalChinese => "繁体字中国語",
            UiText::Grid => "グリッド",
            UiText::GridOriginX => "X 原点",
            UiText::GridOriginY => "Y 原点",
            UiText::GridSpacingX => "X 間隔",
            UiText::GridSpacingY => "Y 間隔",
            UiText::SnapToGrid => "グリッドにスナップ",
            UiText::On => "オン",
            UiText::Off => "オフ",
            UiText::KageRecord => "KAGE レコード",
            UiText::ConnectedGeometry => "連結ジオメトリ",
            UiText::Operation => "操作",
            UiText::Review => "要確認",
            UiText::Neutral => "標準",
            UiText::FlipHorizontal => "左右反転",
            UiText::FlipVertical => "上下反転",
            UiText::RotateLeft => "左に回転",
            UiText::RotateRight => "右に回転",
            UiText::Matches => "件一致",
            UiText::FindDescendants => "⇧ クリック · 子部品を検索",
            UiText::SnapPointerHint => "ポインタ操作による編集に適用",
            UiText::EngineOnline => "エンジン稼働中",
            UiText::StrokeBend => "折れ線",
            UiText::StrokeCorner => "かぎ線",
            UiText::StrokeBezier => "ベジェ",
            UiText::StrokeSweep => "払い",
        },
        UiLanguage::Korean => match key {
            UiText::AppName => "KAGE Editor",
            UiText::UntitledDocument => "제목 없음 · 200 × 200",
            UiText::ReadyEngineConnected => "준비됨 · KAGE 엔진 연결됨",
            UiText::Clean => "변경 없음",
            UiText::Done => "완료",
            UiText::Import => "가져오기",
            UiText::ExportKage => "KAGE 내보내기",
            UiText::CopyAll => "모두 복사",
            UiText::Copied => "복사됨",
            UiText::FilteredKageHint => "내보낼 수 있는 KAGE 레코드만 포함됩니다.",
            UiText::Cut => "오려두기",
            UiText::Copy => "복사",
            UiText::Paste => "붙여넣기",
            UiText::Delete => "삭제",
            UiText::SelectionTool | UiText::Selection => "선택",
            UiText::FreehandTool => "잉크",
            UiText::AddLine => "직선",
            UiText::AddCurve => "곡선",
            UiText::Inspector => "속성",
            UiText::Components | UiText::Component => "구성 요소",
            UiText::Layers => "레이어",
            UiText::NoSelection => "선택 항목 없음",
            UiText::StrokeType => "획 종류",
            UiText::Style => "스타일",
            UiText::Type => "유형",
            UiText::Head => "시작",
            UiText::Tail => "끝",
            UiText::Bounds => "경계",
            UiText::Points => "제어점",
            UiText::Transform => "변형",
            UiText::Metadata => "메타데이터",
            UiText::EngineTransform => "엔진 변형",
            UiText::DocumentHealth => "문서 상태",
            UiText::NoStructuralIssues => "구조 문제가 없습니다",
            UiText::Name => "이름",
            UiText::Stretch => "늘이기",
            UiText::DecomposeToStrokes => "획으로 분해",
            UiText::SearchComponentsPlaceholder => "부수, 이름 또는 Unicode 검색…",
            UiText::NoComponentMatches => "일치하는 구성 요소 없음",
            UiText::PaintOrder => "KAGE 레코드 순서 · 처음부터 끝까지",
            UiText::NoLayers => "이 글리프에는 레이어가 없습니다",
            UiText::BringForward => "앞으로 가져오기",
            UiText::SendBackward => "뒤로 보내기",
            UiText::DocumentAppearance => "문서 모양",
            UiText::Typeface => "서체",
            UiText::Mincho => "명조체",
            UiText::Gothic => "고딕체",
            UiText::Skeleton => "골격만",
            UiText::SmoothStrokes => "부드러운 획 · 명조체 전용",
            UiText::Centerlines => "중심선",
            UiText::CenterlineNone | UiText::MaskNone => "없음",
            UiText::CenterlineSelection => "선택 항목",
            UiText::CenterlineAlways => "항상",
            UiText::Mask => "마스크",
            UiText::MaskCircle => "원",
            UiText::MaskRoundedSquare => "둥근 사각형",
            UiText::MaskSquare => "사각형",
            UiText::MaskDiamond => "마름모",
            UiText::Language => "언어",
            UiText::English => "영어",
            UiText::Japanese => "일본어",
            UiText::Korean => "한국어",
            UiText::SimplifiedChinese => "중국어 간체",
            UiText::TraditionalChinese => "중국어 번체",
            UiText::Grid => "격자",
            UiText::GridOriginX => "X 원점",
            UiText::GridOriginY => "Y 원점",
            UiText::GridSpacingX => "X 간격",
            UiText::GridSpacingY => "Y 간격",
            UiText::SnapToGrid => "격자에 맞추기",
            UiText::On => "켬",
            UiText::Off => "끔",
            UiText::KageRecord => "KAGE 레코드",
            UiText::ConnectedGeometry => "연결된 도형",
            UiText::Operation => "작업",
            UiText::Review => "확인 필요",
            UiText::Neutral => "기본",
            UiText::FlipHorizontal => "좌우 뒤집기",
            UiText::FlipVertical => "상하 뒤집기",
            UiText::RotateLeft => "왼쪽 회전",
            UiText::RotateRight => "오른쪽 회전",
            UiText::Matches => "개 일치",
            UiText::FindDescendants => "⇧ 클릭 · 하위 구성 요소 찾기",
            UiText::SnapPointerHint => "포인터로 편집할 때 적용",
            UiText::EngineOnline => "엔진 온라인",
            UiText::StrokeBend => "꺾은선",
            UiText::StrokeCorner => "모서리선",
            UiText::StrokeBezier => "베지어",
            UiText::StrokeSweep => "삐침",
        },
        UiLanguage::SimplifiedChinese => match key {
            UiText::AppName => "KAGE Editor",
            UiText::UntitledDocument => "未命名 · 200 × 200",
            UiText::ReadyEngineConnected => "就绪 · KAGE 引擎已连接",
            UiText::Clean => "无更改",
            UiText::Done => "完成",
            UiText::Import => "导入",
            UiText::ExportKage => "导出 KAGE",
            UiText::CopyAll => "复制全部",
            UiText::Copied => "已复制",
            UiText::FilteredKageHint => "仅包含可导出的 KAGE 记录。",
            UiText::Cut => "剪切",
            UiText::Copy => "复制",
            UiText::Paste => "粘贴",
            UiText::Delete => "删除",
            UiText::SelectionTool | UiText::Selection => "选择",
            UiText::FreehandTool => "墨迹",
            UiText::AddLine => "直线",
            UiText::AddCurve => "曲线",
            UiText::Inspector => "检查器",
            UiText::Components | UiText::Component => "部件",
            UiText::Layers => "图层",
            UiText::NoSelection => "未选择任何内容",
            UiText::StrokeType => "笔画类型",
            UiText::Style => "样式",
            UiText::Type => "类型",
            UiText::Head => "起笔",
            UiText::Tail => "收笔",
            UiText::Bounds => "边界",
            UiText::Points => "控制点",
            UiText::Transform => "变换",
            UiText::Metadata => "元数据",
            UiText::EngineTransform => "引擎变换",
            UiText::DocumentHealth => "文档状态",
            UiText::NoStructuralIssues => "没有结构问题",
            UiText::Name => "名称",
            UiText::Stretch => "拉伸",
            UiText::DecomposeToStrokes => "分解为笔画",
            UiText::SearchComponentsPlaceholder => "搜索部首、名称或 Unicode…",
            UiText::NoComponentMatches => "没有匹配的部件",
            UiText::PaintOrder => "KAGE 记录顺序 · 先来后到",
            UiText::NoLayers => "此字形没有图层",
            UiText::BringForward => "移至前景一层",
            UiText::SendBackward => "移至背景一层",
            UiText::DocumentAppearance => "文档外观",
            UiText::Typeface => "字体",
            UiText::Mincho => "明朝体",
            UiText::Gothic => "黑体",
            UiText::Skeleton => "仅骨架",
            UiText::SmoothStrokes => "平滑笔画 · 仅限明朝体",
            UiText::Centerlines => "中心线",
            UiText::CenterlineNone | UiText::MaskNone => "无",
            UiText::CenterlineSelection => "仅选择项",
            UiText::CenterlineAlways => "始终显示",
            UiText::Mask => "遮罩",
            UiText::MaskCircle => "圆形",
            UiText::MaskRoundedSquare => "圆角方形",
            UiText::MaskSquare => "方形",
            UiText::MaskDiamond => "菱形",
            UiText::Language => "语言",
            UiText::English => "英语",
            UiText::Japanese => "日语",
            UiText::Korean => "韩语",
            UiText::SimplifiedChinese => "简体中文",
            UiText::TraditionalChinese => "繁体中文",
            UiText::Grid => "网格",
            UiText::GridOriginX => "X 原点",
            UiText::GridOriginY => "Y 原点",
            UiText::GridSpacingX => "X 间距",
            UiText::GridSpacingY => "Y 间距",
            UiText::SnapToGrid => "吸附到网格",
            UiText::On => "开启",
            UiText::Off => "关闭",
            UiText::KageRecord => "KAGE 记录",
            UiText::ConnectedGeometry => "连通几何",
            UiText::Operation => "操作",
            UiText::Review => "需检查",
            UiText::Neutral => "默认",
            UiText::FlipHorizontal => "左右翻转",
            UiText::FlipVertical => "上下翻转",
            UiText::RotateLeft => "向左旋转",
            UiText::RotateRight => "向右旋转",
            UiText::Matches => "个匹配",
            UiText::FindDescendants => "⇧ 点击 · 查找子部件",
            UiText::SnapPointerHint => "适用于指针编辑",
            UiText::EngineOnline => "引擎在线",
            UiText::StrokeBend => "折线",
            UiText::StrokeCorner => "转角线",
            UiText::StrokeBezier => "贝塞尔",
            UiText::StrokeSweep => "撇线",
        },
        UiLanguage::TraditionalChinese => match key {
            UiText::AppName => "KAGE Editor",
            UiText::UntitledDocument => "未命名 · 200 × 200",
            UiText::ReadyEngineConnected => "就緒 · KAGE 引擎已連線",
            UiText::Clean => "無更改",
            UiText::Done => "完成",
            UiText::Import => "匯入",
            UiText::ExportKage => "匯出 KAGE",
            UiText::CopyAll => "複製全部",
            UiText::Copied => "已複製",
            UiText::FilteredKageHint => "僅包含可匯出的 KAGE 記錄。",
            UiText::Cut => "剪下",
            UiText::Copy => "複製",
            UiText::Paste => "貼上",
            UiText::Delete => "刪除",
            UiText::SelectionTool => "選取",
            UiText::FreehandTool => "墨跡",
            UiText::AddLine => "直線",
            UiText::AddCurve => "曲線",
            UiText::Inspector => "檢查器",
            UiText::Components | UiText::Component => "部件",
            UiText::Layers => "圖層",
            UiText::Selection => "選取範圍",
            UiText::NoSelection => "尚未選取任何內容",
            UiText::StrokeType => "筆畫類型",
            UiText::Style => "樣式",
            UiText::Type => "類型",
            UiText::Head => "起筆",
            UiText::Tail => "收筆",
            UiText::Bounds => "邊界",
            UiText::Points => "控制點",
            UiText::Transform => "變換",
            UiText::Metadata => "中繼資料",
            UiText::EngineTransform => "引擎變換",
            UiText::DocumentHealth => "文件狀態",
            UiText::NoStructuralIssues => "沒有結構問題",
            UiText::Name => "名稱",
            UiText::Stretch => "拉伸",
            UiText::DecomposeToStrokes => "分解為筆畫",
            UiText::SearchComponentsPlaceholder => "搜尋部首、名稱或 Unicode…",
            UiText::NoComponentMatches => "沒有相符的部件",
            UiText::PaintOrder => "KAGE 記錄順序 · 先來後到",
            UiText::NoLayers => "此字形沒有圖層",
            UiText::BringForward => "移至前景一層",
            UiText::SendBackward => "移至背景一層",
            UiText::DocumentAppearance => "文件外觀",
            UiText::Typeface => "字體",
            UiText::Mincho => "明朝體",
            UiText::Gothic => "黑體",
            UiText::Skeleton => "僅骨架",
            UiText::SmoothStrokes => "圓滑筆畫 · 僅限明朝體",
            UiText::Centerlines => "中心線",
            UiText::CenterlineNone | UiText::MaskNone => "無",
            UiText::CenterlineSelection => "僅選取項目",
            UiText::CenterlineAlways => "永遠顯示",
            UiText::Mask => "遮罩",
            UiText::MaskCircle => "圓形",
            UiText::MaskRoundedSquare => "圓角方形",
            UiText::MaskSquare => "方形",
            UiText::MaskDiamond => "菱形",
            UiText::Language => "語言",
            UiText::English => "英文",
            UiText::Japanese => "日文",
            UiText::Korean => "韓文",
            UiText::SimplifiedChinese => "簡體中文",
            UiText::TraditionalChinese => "繁體中文",
            UiText::Grid => "格線",
            UiText::GridOriginX => "X 原點",
            UiText::GridOriginY => "Y 原點",
            UiText::GridSpacingX => "X 間距",
            UiText::GridSpacingY => "Y 間距",
            UiText::SnapToGrid => "貼齊格線",
            UiText::On => "開啟",
            UiText::Off => "關閉",
            UiText::KageRecord => "KAGE 記錄",
            UiText::ConnectedGeometry => "連接幾何",
            UiText::Operation => "操作",
            UiText::Review => "需要檢查",
            UiText::Neutral => "預設",
            UiText::FlipHorizontal => "左右翻轉",
            UiText::FlipVertical => "上下翻轉",
            UiText::RotateLeft => "向左旋轉",
            UiText::RotateRight => "向右旋轉",
            UiText::Matches => "個相符項目",
            UiText::FindDescendants => "⇧ 點按 · 尋找子部件",
            UiText::SnapPointerHint => "套用至指標驅動的編輯",
            UiText::EngineOnline => "引擎在線",
            UiText::StrokeBend => "折線",
            UiText::StrokeCorner => "轉角線",
            UiText::StrokeBezier => "貝茲曲線",
            UiText::StrokeSweep => "撇線",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{UiText, text};
    use crate::model::UiLanguage;

    const LANGUAGES: [UiLanguage; 5] = [
        UiLanguage::English,
        UiLanguage::Japanese,
        UiLanguage::Korean,
        UiLanguage::SimplifiedChinese,
        UiLanguage::TraditionalChinese,
    ];

    #[test]
    fn every_catalogue_entry_is_non_empty_in_every_language() {
        for language in LANGUAGES {
            for key in UiText::ALL {
                assert!(
                    !text(language, key).trim().is_empty(),
                    "missing {language:?} translation for {key:?}"
                );
            }
        }
    }

    #[test]
    fn representative_interface_copy_is_distinct_between_languages() {
        for key in [
            UiText::DocumentAppearance,
            UiText::FreehandTool,
            UiText::DocumentHealth,
            UiText::SnapToGrid,
            UiText::SmoothStrokes,
        ] {
            let translations = LANGUAGES.map(|language| text(language, key));
            for (index, translation) in translations.iter().enumerate() {
                for other in &translations[index + 1..] {
                    assert_ne!(translation, other, "{key:?} should be localized");
                }
            }
        }
    }

    #[test]
    fn native_language_names_are_available_without_allocation() {
        assert_eq!(text(UiLanguage::Japanese, UiText::Japanese), "日本語");
        assert_eq!(text(UiLanguage::Korean, UiText::Korean), "한국어");
        assert_eq!(
            text(UiLanguage::SimplifiedChinese, UiText::SimplifiedChinese),
            "简体中文"
        );
        assert_eq!(
            text(UiLanguage::TraditionalChinese, UiText::TraditionalChinese),
            "繁體中文"
        );
    }
}
