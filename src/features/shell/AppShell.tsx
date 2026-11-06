import { useCallback, useEffect, useState, type ReactNode } from "react"
import {
  AudioLinesIcon,
  BookOpenTextIcon,
  FilesIcon,
  HistoryIcon,
  MenuIcon,
  Settings2Icon,
  SlidersHorizontalIcon,
  type LucideIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Separator } from "@/components/ui/separator"
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet"
import { cn } from "@/lib/utils"

export type AppRoute = "workbench" | "knowledge" | "history" | "settings"

export interface AppShellProps {
  renderContent?: (route: AppRoute) => ReactNode
}

interface RouteDefinition {
  route: AppRoute
  label: string
  title: string
  description: string
  emptyTitle: string
  emptyDescription: string
  icon: LucideIcon
  emptyIcon: LucideIcon
}

const DEFAULT_ROUTE: AppRoute = "workbench"

const ROUTES: RouteDefinition[] = [
  {
    route: "workbench",
    label: "实时工作台",
    title: "实时工作台",
    description: "在本机完成实时同传，并让原文与译文保持清晰对应。",
    emptyTitle: "尚未开始本机会话",
    emptyDescription: "音频设备、语言和服务就绪后，会话字幕会显示在这里。",
    icon: AudioLinesIcon,
    emptyIcon: AudioLinesIcon,
  },
  {
    route: "knowledge",
    label: "知识库",
    title: "知识库",
    description: "管理仅保存在本机的资料与检索结果。",
    emptyTitle: "知识库尚无文档",
    emptyDescription: "导入的本地文档与检索结果会显示在这里。",
    icon: BookOpenTextIcon,
    emptyIcon: FilesIcon,
  },
  {
    route: "history",
    label: "会话历史",
    title: "会话历史",
    description: "查看已结束并保存在本机的会话。",
    emptyTitle: "还没有会话记录",
    emptyDescription: "完成的本机会话会显示在这里，便于查看、总结与导出。",
    icon: HistoryIcon,
    emptyIcon: HistoryIcon,
  },
  {
    route: "settings",
    label: "设置",
    title: "设置",
    description: "集中管理音频、服务与本地数据选项。",
    emptyTitle: "设置尚未配置",
    emptyDescription: "音频设备、同传服务和本地数据选项会显示在这里。",
    icon: Settings2Icon,
    emptyIcon: SlidersHorizontalIcon,
  },
]

const ROUTE_NAMES = new Set<AppRoute>(ROUTES.map(({ route }) => route))

function routeFromHash(hash: string): AppRoute {
  const candidate = hash.replace(/^#/, "") as AppRoute
  return ROUTE_NAMES.has(candidate) ? candidate : DEFAULT_ROUTE
}

function routeHash(route: AppRoute) {
  return `#${route}`
}

function useHashRoute() {
  const [route, setRoute] = useState<AppRoute>(() =>
    routeFromHash(window.location.hash),
  )

  useEffect(() => {
    const syncRoute = () => {
      const nextRoute = routeFromHash(window.location.hash)
      setRoute(nextRoute)

      if (window.location.hash !== routeHash(nextRoute)) {
        window.history.replaceState(null, "", routeHash(nextRoute))
      }
    }

    syncRoute()
    window.addEventListener("hashchange", syncRoute)
    return () => window.removeEventListener("hashchange", syncRoute)
  }, [])

  const selectRoute = useCallback((nextRoute: AppRoute) => {
    setRoute(nextRoute)
  }, [])

  return { route, selectRoute }
}

function VoxBridgeMark() {
  return (
    <svg
      aria-hidden="true"
      className="app-brand-mark"
      viewBox="0 0 24 24"
      fill="none"
    >
      <path
        d="M3 8.5 7 15l5-9 5 9 4-6.5"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M5.5 18h13"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  )
}

function Brand() {
  return (
    <div className="app-brand" aria-label="Vox Bridge">
      <VoxBridgeMark />
      <span className="app-brand-name">Vox Bridge</span>
    </div>
  )
}

interface NavigationProps {
  activeRoute: AppRoute
  onSelect: (route: AppRoute) => void
  closeOnSelect?: boolean
}

function Navigation({
  activeRoute,
  onSelect,
  closeOnSelect = false,
}: NavigationProps) {
  return (
    <nav className="app-navigation" aria-label="主导航">
      <ul>
        {ROUTES.map(({ route, label, icon: Icon }) => {
          const link = (
            <a
              href={routeHash(route)}
              aria-current={activeRoute === route ? "page" : undefined}
              className={cn(
                "app-nav-link",
                activeRoute === route && "app-nav-link-active",
              )}
              onClick={() => onSelect(route)}
            >
              <Icon aria-hidden="true" />
              <span>{label}</span>
            </a>
          )

          return (
            <li key={route}>
              {closeOnSelect ? <SheetClose asChild>{link}</SheetClose> : link}
            </li>
          )
        })}
      </ul>
    </nav>
  )
}

function DefaultRouteContent({ route }: { route: AppRoute }) {
  const definition = ROUTES.find((item) => item.route === route) ?? ROUTES[0]
  const EmptyIcon = definition.emptyIcon

  return (
    <Empty className="app-empty-state">
      <EmptyHeader>
        <EmptyMedia>
          <EmptyIcon aria-hidden="true" />
        </EmptyMedia>
        <EmptyTitle>{definition.emptyTitle}</EmptyTitle>
        <EmptyDescription>{definition.emptyDescription}</EmptyDescription>
      </EmptyHeader>
      {route === "workbench" && (
        <EmptyContent>
          <Button asChild className="app-empty-action min-h-11 px-4">
            <a href="#settings">检查设置</a>
          </Button>
        </EmptyContent>
      )}
    </Empty>
  )
}

export function AppShell({ renderContent }: AppShellProps) {
  const { route, selectRoute } = useHashRoute()
  const currentRoute = ROUTES.find((item) => item.route === route) ?? ROUTES[0]

  return (
    <div className="app-shell">
      <aside className="app-sidebar" aria-label="应用导航">
        <Brand />
        <Navigation activeRoute={route} onSelect={selectRoute} />
        <div className="app-sidebar-scope">
          <div className="app-scope-label">当前范围</div>
          <div className="app-scope-value">本机会话</div>
        </div>
      </aside>

      <header className="app-topbar">
        <Sheet>
          <SheetTrigger asChild>
            <Button
              className="app-mobile-trigger"
              variant="ghost"
              aria-label="打开导航菜单"
            >
              <MenuIcon data-icon="inline-start" />
              <span className="app-mobile-brand">Vox Bridge</span>
            </Button>
          </SheetTrigger>
          <SheetContent
            side="left"
            className="app-nav-sheet !w-full !max-w-none"
          >
            <SheetHeader>
              <SheetTitle>导航</SheetTitle>
              <SheetDescription>选择 Vox Bridge 工作区。</SheetDescription>
            </SheetHeader>
            <div className="app-nav-sheet-body">
              <Navigation
                activeRoute={route}
                onSelect={selectRoute}
                closeOnSelect
              />
              <div className="app-sidebar-scope">
                <div className="app-scope-label">当前范围</div>
                <div className="app-scope-value">本机会话</div>
              </div>
            </div>
          </SheetContent>
        </Sheet>

        <div className="app-topbar-page">{currentRoute.label}</div>
        <div className="app-topbar-scope">本机会话</div>
      </header>

      <main className="app-sheet">
        <div className="app-page">
          <header className="app-page-header">
            <h1>{currentRoute.title}</h1>
            <p>{currentRoute.description}</p>
          </header>
          <Separator className="app-page-separator" />
          <section className="app-route-slot" aria-label={`${currentRoute.title}内容`}>
            {renderContent?.(route) ?? <DefaultRouteContent route={route} />}
          </section>
        </div>
      </main>
    </div>
  )
}
