import { defineConfig } from 'vitepress'

const referenceSidebar = [
  {
    text: '參考資料',
    items: [
      { text: '參考總覽', link: '/reference/' },
      { text: '架構決策', link: '/architecture' },
      { text: '能力矩陣', link: '/capability-matrix' },
    ],
  },
]

const guideSidebar = [
  {
    text: '開始使用',
    items: [
      { text: '介紹', link: '/guide/introduction' },
      { text: '快速開始', link: '/guide/quick-start' },
    ],
  },
  {
    text: '基礎',
    collapsed: false,
    items: [
      { text: '應用程式與視窗', link: '/guide/essentials/application' },
      { text: '模板語法', link: '/guide/essentials/template-syntax' },
      { text: '反應式狀態', link: '/guide/essentials/reactivity-fundamentals' },
      { text: '計算與快取', link: '/guide/essentials/computed' },
      { text: 'Class 與 Style', link: '/guide/essentials/class-and-style' },
      { text: '條件渲染', link: '/guide/essentials/conditional' },
      { text: '列表渲染', link: '/guide/essentials/list' },
      { text: '事件處理', link: '/guide/essentials/event-handling' },
      { text: '表單與文字輸入', link: '/guide/essentials/forms' },
      { text: 'Observers 與 Effects', link: '/guide/essentials/watchers' },
      { text: '原生 Refs', link: '/guide/essentials/template-refs' },
      { text: '元件基礎', link: '/guide/essentials/component-basics' },
      { text: '生命週期', link: '/guide/essentials/lifecycle' },
    ],
  },
  {
    text: '深入元件',
    collapsed: true,
    items: [
      { text: '宣告與使用', link: '/guide/components/registration' },
      { text: 'Props', link: '/guide/components/props' },
      { text: 'Events', link: '/guide/components/events' },
      { text: '雙向綁定', link: '/guide/components/v-model' },
      { text: '屬性轉送', link: '/guide/components/attrs' },
      { text: 'Slots', link: '/guide/components/slots' },
      { text: 'Provide / Inject', link: '/guide/components/provide-inject' },
      { text: '非同步元件', link: '/guide/components/async' },
    ],
  },
  {
    text: '重用',
    collapsed: true,
    items: [
      { text: '可重用狀態與行為', link: '/guide/reusability/composables' },
      { text: '自訂原生行為', link: '/guide/reusability/custom-directives' },
      { text: '外掛', link: '/guide/reusability/plugins' },
    ],
  },
  {
    text: '原生 Built-ins',
    collapsed: true,
    items: [
      { text: 'Transition', link: '/guide/built-ins/transition' },
      { text: 'TransitionGroup', link: '/guide/built-ins/transition-group' },
      { text: 'KeepAlive', link: '/guide/built-ins/keep-alive' },
      { text: 'Overlays 與 Portals', link: '/guide/built-ins/teleport' },
      { text: 'Async Boundaries', link: '/guide/built-ins/suspense' },
    ],
  },
  {
    text: '大型應用',
    collapsed: true,
    items: [
      { text: 'Rust 元件模組', link: '/guide/scaling-up/sfc' },
      { text: '工具鏈', link: '/guide/scaling-up/tooling' },
      { text: '畫面路由', link: '/guide/scaling-up/routing' },
      { text: '狀態管理', link: '/guide/scaling-up/state-management' },
      { text: '測試', link: '/guide/scaling-up/testing' },
      { text: '為何 SSR 不適用', link: '/guide/scaling-up/ssr' },
    ],
  },
  {
    text: '最佳實務',
    collapsed: true,
    items: [
      { text: '發布桌面應用', link: '/guide/best-practices/production-deployment' },
      { text: '效能', link: '/guide/best-practices/performance' },
      { text: '無障礙', link: '/guide/best-practices/accessibility' },
      { text: '安全性', link: '/guide/best-practices/security' },
    ],
  },
  {
    text: 'Rust 型別',
    collapsed: true,
    items: [
      { text: '型別系統總覽', link: '/guide/typescript/overview' },
      { text: '組合式寫法', link: '/guide/typescript/composition-api' },
      { text: 'component! 型別', link: '/guide/typescript/options-api' },
    ],
  },
  {
    text: '延伸主題',
    collapsed: true,
    items: [
      { text: '採用方式', link: '/guide/extras/ways-of-using-gpui-vue' },
      { text: 'Component API FAQ', link: '/guide/extras/component-api-faq' },
      { text: '反應式原理', link: '/guide/extras/reactivity-in-depth' },
      { text: '渲染機制', link: '/guide/extras/rendering-mechanism' },
      { text: 'Render 與 Builders', link: '/guide/extras/render-function' },
      { text: 'Web Components 邊界', link: '/guide/extras/web-components' },
      { text: '動畫', link: '/guide/extras/animation' },
    ],
  },
]

const apiSidebar = [
  {
    text: 'API 參考',
    items: [
      { text: 'API 總覽', link: '/api/' },
      { text: '通用 API', link: '/api/general' },
      { text: 'Application', link: '/api/application' },
    ],
  },
  {
    text: 'Composition API',
    collapsed: false,
    items: [
      { text: 'Setup', link: '/api/composition-api-setup' },
      { text: 'Reactivity: Core', link: '/api/reactivity-core' },
      { text: 'Reactivity: Utilities', link: '/api/reactivity-utilities' },
      { text: 'Reactivity: Advanced', link: '/api/reactivity-advanced' },
      { text: 'Lifecycle', link: '/api/composition-api-lifecycle' },
      { text: 'App-wide State', link: '/api/composition-api-dependency-injection' },
      { text: 'Helpers', link: '/api/composition-api-helpers' },
    ],
  },
  {
    text: 'Options API',
    collapsed: true,
    items: [
      { text: 'State', link: '/api/options-state' },
      { text: 'Rendering', link: '/api/options-rendering' },
      { text: 'Lifecycle', link: '/api/options-lifecycle' },
      { text: 'Composition', link: '/api/options-composition' },
      { text: 'Miscellaneous', link: '/api/options-misc' },
    ],
  },
  {
    text: 'Built-ins',
    collapsed: true,
    items: [
      { text: 'Components', link: '/api/built-in-components' },
      { text: 'Directives', link: '/api/built-in-directives' },
      { text: 'Special Elements', link: '/api/built-in-special-elements' },
      { text: 'Special Attributes', link: '/api/built-in-special-attributes' },
    ],
  },
  {
    text: '元件與渲染',
    collapsed: true,
    items: [
      { text: 'Component Setup DSL', link: '/api/component-setup' },
      { text: 'Component Instance', link: '/api/component-instance' },
      { text: 'Render Function', link: '/api/render-function' },
      { text: 'Native Style Features', link: '/api/native-style-features' },
      { text: 'Custom Renderer Boundary', link: '/api/custom-renderer' },
      { text: 'Custom Elements Boundary', link: '/api/custom-elements' },
    ],
  },
  {
    text: '語言、型別與平台',
    collapsed: true,
    items: [
      { text: 'Rust Component Files', link: '/api/sfc-spec' },
      { text: 'Utility Types', link: '/api/utility-types' },
      { text: 'Compile-time Flags', link: '/api/compile-time-flags' },
      { text: 'SSR Boundary', link: '/api/ssr' },
    ],
  },
]

export default defineConfig({
  lang: 'zh-Hant',
  title: 'gpui-vue',
  description: '以 Vue 啟發的 Rust 編譯期語法，直接構建原生 GPUI 桌面介面。',
  cleanUrls: true,
  lastUpdated: true,
  ignoreDeadLinks: false,
  head: [
    ['meta', { name: 'theme-color', content: '#17181b' }],
    ['meta', { name: 'color-scheme', content: 'light dark' }],
  ],
  markdown: {
    theme: {
      light: 'github-light',
      dark: 'github-dark',
    },
  },
  themeConfig: {
    nav: [
      { text: '指南', link: '/guide/introduction' },
      { text: 'API', link: '/api/' },
      { text: '範例', link: '/examples/counter' },
      { text: '參考', link: '/reference/' },
    ],
    sidebar: {
      '/guide/': guideSidebar,
      '/api/': apiSidebar,
      '/examples/': [
        {
          text: '完整範例',
          items: [
            { text: 'Counter', link: '/examples/counter' },
            { text: 'KAGE Editor', link: '/examples/kage-editor' },
          ],
        },
      ],
      '/reference/': referenceSidebar,
      '/architecture': referenceSidebar,
      '/capability-matrix': referenceSidebar,
    },
    search: {
      provider: 'local',
    },
    outline: {
      level: [2, 3],
      label: '本頁導覽',
    },
    docFooter: {
      prev: '上一頁',
      next: '下一頁',
    },
    lastUpdated: {
      text: '最後更新',
    },
    sidebarMenuLabel: '目錄',
    returnToTopLabel: '回到頂端',
    darkModeSwitchLabel: '外觀',
    socialLinks: [
      { icon: 'github', link: 'https://github.com/wcshds/gpui-vue' },
    ],
    footer: {
      message: '以 Rust 編譯，以 GPUI 呈現。',
      copyright: 'Released under the MIT or Apache-2.0 license.',
    },
  },
})
