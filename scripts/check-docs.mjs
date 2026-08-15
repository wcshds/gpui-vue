import { readFile, readdir, stat } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url))
const repositoryRoot = path.resolve(scriptDirectory, '..')
const docsRoot = path.join(repositoryRoot, 'docs')
const publicRoot = path.join(docsRoot, 'public')
const vitePressConfig = path.join(docsRoot, '.vitepress', 'config.mts')

// These files are deliberate top-level destinations rather than documentation
// pages that need to appear in a section index or sidebar.
const orphanExclusions = new Set([
  'index.md',
  'architecture.md',
  'capability-matrix.md',
])

const errors = []

function relativeToRepository(file) {
  return path.relative(repositoryRoot, file).split(path.sep).join('/')
}

function relativeToDocs(file) {
  return path.relative(docsRoot, file).split(path.sep).join('/')
}

async function isFile(file) {
  try {
    return (await stat(file)).isFile()
  } catch {
    return false
  }
}

async function findMarkdownFiles(directory) {
  const files = []

  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name === '.vitepress') continue

    const candidate = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...await findMarkdownFiles(candidate))
    } else if (entry.isFile() && entry.name.endsWith('.md')) {
      files.push(candidate)
    }
  }

  return files.sort()
}

async function findVueFiles(directory) {
  const files = []

  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (
      entry.isDirectory()
      && path.basename(directory) === '.vitepress'
      && (entry.name === 'cache' || entry.name === 'dist')
    ) {
      continue
    }

    const candidate = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...await findVueFiles(candidate))
    } else if (entry.isFile() && entry.name.endsWith('.vue')) {
      files.push(candidate)
    }
  }

  return files.sort()
}

function stripQueryAndFragment(value) {
  return value.split(/[?#]/, 1)[0]
}

function isExternalLink(value) {
  return /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(value)
}

async function resolveMarkdownRoute(route, sourceFile = undefined) {
  if (!route || route.startsWith('#') || isExternalLink(route)) return undefined

  const cleanRoute = stripQueryAndFragment(route)
  if (!cleanRoute) return sourceFile

  let routePath
  if (cleanRoute.startsWith('/')) {
    routePath = path.join(docsRoot, cleanRoute.slice(1))
  } else {
    routePath = path.resolve(path.dirname(sourceFile ?? path.join(docsRoot, 'index.md')), cleanRoute)
  }

  const relative = path.relative(docsRoot, routePath)
  if (relative.startsWith('..') || path.isAbsolute(relative)) return undefined

  if (routePath.endsWith('.html')) {
    routePath = `${routePath.slice(0, -'.html'.length)}.md`
  }

  const extension = path.extname(routePath)
  if (extension && extension !== '.md') return undefined

  const candidates = extension === '.md'
    ? [routePath]
    : cleanRoute.endsWith('/') || cleanRoute === '/'
      ? [path.join(routePath, 'index.md')]
      : [`${routePath}.md`, path.join(routePath, 'index.md')]

  for (const candidate of candidates) {
    if (await isFile(candidate)) return path.normalize(candidate)
  }

  return null
}

function extractConfigRoutes(source) {
  const routes = []
  const linkPattern = /\blink\s*:\s*(['"`])([^'"`]+)\1/g

  for (const match of source.matchAll(linkPattern)) {
    const route = match[2].trim()
    if (route.startsWith('/') && !route.startsWith('//')) routes.push(route)
  }

  return [...new Set(routes)]
}

function sourceWithoutFencedCode(source) {
  return source.replace(
    /^\s*(```|~~~)[\s\S]*?^\s*\1.*$/gm,
    (fence) => fence.replace(/[^\n]/g, ' '),
  )
}

function extractMarkdownLinks(source) {
  const links = []
  const prose = sourceWithoutFencedCode(source)
  const inlineLink = /(?<!!)\[[^\]]*\]\(\s*([^\s)]+)(?:\s+['"][^)]*['"])?\s*\)/g
  const referenceLink = /^\s*\[[^\]]+\]:\s*(\S+)/gm

  for (const match of prose.matchAll(inlineLink)) links.push(match[1])
  for (const match of prose.matchAll(referenceLink)) links.push(match[1])

  return links
}

function extractStaticAssetReferences(source, isMarkdown) {
  const references = []
  const searchableSource = isMarkdown ? sourceWithoutFencedCode(source) : source
  const patterns = [
    /!\[[^\]]*\]\(\s*<?(\/(?!\/)[^\s)>]+)>?(?:\s+['"][^)]*['"])?\s*\)/g,
    /\b(?:src|poster)\s*=\s*(['"])(\/(?!\/)[^'"]+)\1/g,
    /\burl\(\s*(['"]?)(\/(?!\/)[^'"\s)]+)\1\s*\)/g,
  ]

  patterns.forEach((pattern, patternIndex) => {
    for (const match of searchableSource.matchAll(pattern)) {
      const assetPath = patternIndex === 0 ? match[1] : match[2]
      references.push({
        assetPath,
        line: searchableSource.slice(0, match.index).split('\n').length,
      })
    }
  })

  return references
}

async function checkStaticAssets(contentFiles) {
  let assetCount = 0

  for (const contentFile of contentFiles) {
    const source = await readFile(contentFile, 'utf8')
    const isMarkdown = contentFile.endsWith('.md')

    for (const reference of extractStaticAssetReferences(source, isMarkdown)) {
      assetCount += 1
      let cleanAssetPath
      try {
        cleanAssetPath = decodeURIComponent(stripQueryAndFragment(reference.assetPath))
      } catch {
        errors.push(`${relativeToRepository(contentFile)}:${reference.line}: 靜態資產路徑含有無效 URL 編碼：${reference.assetPath}`)
        continue
      }

      const assetFile = path.join(publicRoot, cleanAssetPath.slice(1))
      const relative = path.relative(publicRoot, assetFile)
      if (relative.startsWith('..') || path.isAbsolute(relative) || !await isFile(assetFile)) {
        errors.push(`${relativeToRepository(contentFile)}:${reference.line}: root-relative 靜態資產不存在：docs/public${cleanAssetPath}`)
      }
    }
  }

  return assetCount
}

function parseIncludeSpecification(specification) {
  let value = specification.trim()
  value = value.replace(/\{[^{}]*\}\s*$/, '').trim()

  const fragmentIndex = value.lastIndexOf('#')
  if (fragmentIndex === -1) return { sourcePath: value, region: undefined }

  return {
    sourcePath: value.slice(0, fragmentIndex),
    region: value.slice(fragmentIndex + 1),
  }
}

function extractIncludes(source) {
  const includes = []
  const includePattern = /^\s*<<<\s+(.+?)\s*$/gm

  for (const match of source.matchAll(includePattern)) {
    includes.push({
      ...parseIncludeSpecification(match[1]),
      line: source.slice(0, match.index).split('\n').length,
    })
  }

  return includes
}

function parseRegions(source, sourceFile) {
  const regions = new Map()
  const stack = []
  const markerPattern = /^\s*(?:(?:\/\/|#|\/\*+|<!--)\s*)?#(end)?region(?:\s+([A-Za-z\d_.-]+))?/i

  source.split('\n').forEach((line, index) => {
    const match = markerPattern.exec(line)
    if (!match) return

    const lineNumber = index + 1
    const isEnd = Boolean(match[1])
    const name = match[2]

    if (!isEnd) {
      if (!name) {
        errors.push(`${relativeToRepository(sourceFile)}:${lineNumber}: region 開始標記缺少名稱`)
        return
      }
      stack.push({ name, line: lineNumber })
      return
    }

    const start = stack.pop()
    if (!start) {
      errors.push(`${relativeToRepository(sourceFile)}:${lineNumber}: 找不到對應開始標記的 #endregion`)
      return
    }

    if (name && name !== start.name) {
      errors.push(`${relativeToRepository(sourceFile)}:${lineNumber}: #endregion ${name} 與第 ${start.line} 行的 #region ${start.name} 不成對`)
      return
    }

    if (regions.has(start.name)) {
      errors.push(`${relativeToRepository(sourceFile)}:${start.line}: region ${start.name} 在同一檔案重複定義`)
      return
    }

    regions.set(start.name, { start: start.line, end: lineNumber })
  })

  for (const start of stack) {
    errors.push(`${relativeToRepository(sourceFile)}:${start.line}: region ${start.name} 缺少 #endregion`)
  }

  return regions
}

async function checkCodeIncludes(markdownFiles) {
  const regionCache = new Map()
  let includeCount = 0

  for (const markdownFile of markdownFiles) {
    const markdown = await readFile(markdownFile, 'utf8')

    for (const include of extractIncludes(markdown)) {
      includeCount += 1
      let sourcePath = include.sourcePath
      if (sourcePath.startsWith('@/')) sourcePath = path.join(docsRoot, sourcePath.slice(2))
      else sourcePath = path.resolve(path.dirname(markdownFile), sourcePath)

      if (!await isFile(sourcePath)) {
        errors.push(`${relativeToRepository(markdownFile)}:${include.line}: include 檔案不存在：${relativeToRepository(sourcePath)}`)
        continue
      }

      if (!include.region) continue

      if (!regionCache.has(sourcePath)) {
        const source = await readFile(sourcePath, 'utf8')
        regionCache.set(sourcePath, parseRegions(source, sourcePath))
      }

      if (!regionCache.get(sourcePath).has(include.region)) {
        errors.push(`${relativeToRepository(markdownFile)}:${include.line}: ${relativeToRepository(sourcePath)} 缺少成對 region：${include.region}`)
      }
    }
  }

  return includeCount
}

async function checkConfigRoutes(configRoutes) {
  const routeFiles = new Map()

  for (const route of configRoutes) {
    const target = await resolveMarkdownRoute(route)
    if (target === null) {
      errors.push(`docs/.vitepress/config.mts: 路由 ${route} 沒有對應的 Markdown 檔案`)
    } else if (target) {
      routeFiles.set(route, target)
    }
  }

  return routeFiles
}

async function findReachablePages(markdownFiles, routeFiles) {
  const markdownSet = new Set(markdownFiles.map(path.normalize))
  const visited = new Set()
  const queue = [path.join(docsRoot, 'index.md'), ...routeFiles.values()]

  while (queue.length > 0) {
    const current = path.normalize(queue.shift())
    if (visited.has(current) || !markdownSet.has(current)) continue
    visited.add(current)

    const source = await readFile(current, 'utf8')
    for (const link of extractMarkdownLinks(source)) {
      const target = await resolveMarkdownRoute(link, current)
      if (target && !visited.has(target)) queue.push(target)
    }
  }

  const orphans = markdownFiles.filter((file) => {
    return !visited.has(path.normalize(file)) && !orphanExclusions.has(relativeToDocs(file))
  })

  for (const orphan of orphans) {
    errors.push(`${relativeToRepository(orphan)}: 無法從 nav、sidebar 或文件索引抵達`)
  }

  return { visited, orphans }
}

const markdownFiles = await findMarkdownFiles(docsRoot)
const vueFiles = await findVueFiles(docsRoot)
const configSource = await readFile(vitePressConfig, 'utf8')
const configRoutes = extractConfigRoutes(configSource)
const includeCount = await checkCodeIncludes(markdownFiles)
const assetCount = await checkStaticAssets([...markdownFiles, ...vueFiles])
const routeFiles = await checkConfigRoutes(configRoutes)
const { visited } = await findReachablePages(markdownFiles, routeFiles)

if (errors.length > 0) {
  console.error(`文檔檢查失敗（${errors.length} 項）：`)
  for (const error of errors) console.error(`  - ${error}`)
  process.exitCode = 1
} else {
  console.log(`文檔檢查通過：${markdownFiles.length} 個 Markdown 頁面、${configRoutes.length} 條配置路由、${includeCount} 個程式碼引用、${assetCount} 個靜態資產引用、${visited.size} 個可抵達頁面。`)
}
