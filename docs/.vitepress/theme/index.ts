import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import NativeResult from './NativeResult.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('NativeResult', NativeResult)
  },
} satisfies Theme
