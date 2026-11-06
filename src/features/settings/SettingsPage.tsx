import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  Empty,
  EmptyHeader,
  EmptyTitle,
  EmptyDescription,
} from "@/components/ui/empty";
import { AudioSettings } from "./AudioSettings";
import { ProviderSettings } from "./ProviderSettings";
import type { Workbench } from "@/features/workbench/useWorkbench";
export function SettingsPage({ owner }: { owner: Workbench }) {
  return (
    <Tabs defaultValue="audio" className="settings-page">
      <TabsList variant="line" className="settings-tabs">
        <TabsTrigger value="audio">音频</TabsTrigger>
        <TabsTrigger value="provider">同传服务</TabsTrigger>
        <TabsTrigger value="models">模型</TabsTrigger>
        <TabsTrigger value="research">客户研究</TabsTrigger>
        <TabsTrigger value="data">本地数据</TabsTrigger>
      </TabsList>
      <TabsContent value="audio">
        <AudioSettings owner={owner} />
      </TabsContent>
      <TabsContent value="provider">
        <ProviderSettings owner={owner} />
      </TabsContent>
      {[
        {
          id: "models",
          title: "模型尚未配置",
          description: "配置 LLM 与嵌入模型后，可使用摘要和本地知识检索。",
        },
        {
          id: "research",
          title: "客户研究尚未配置",
          description: "配置 Tavily 后，可发起客户研究。",
        },
        {
          id: "data",
          title: "本地数据",
          description: "字幕、诊断和检索结果保存在本机。数据位置管理尚未开放。",
        },
      ].map((x) => (
        <TabsContent key={x.id} value={x.id}>
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{x.title}</EmptyTitle>
              <EmptyDescription>{x.description}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        </TabsContent>
      ))}
    </Tabs>
  );
}
