// 查重源 / 通用模板库（本地持久化）。命中这些样板的段落在查重时被剔除。
export interface Template {
  id: string;
  name: string;
  text: string;
}

const KEY = "bidguard-templates";

const DEFAULTS: Template[] = [
  {
    id: "t-law",
    name: "法律法规引用",
    text: "根据《中华人民共和国招标投标法》及其实施条例，以及《中华人民共和国政府采购法》的相关规定，本项目严格遵循公开、公平、公正和诚实信用的原则组织实施。",
  },
  {
    id: "t-qual",
    name: "资质证书目录",
    text: "投标人具备独立法人资格，持有有效的营业执照、税务登记证及与本项目相适应的行业资质证书与质量管理体系认证，所有证照均在有效期内。",
  },
  {
    id: "t-after",
    name: "标准售后承诺",
    text: "我方承诺提供 7×24 小时技术支持服务，质保期内免费维护，接到故障报修后及时响应并在约定时限内解决，确保系统稳定运行。",
  },
];

export function loadTemplates(): Template[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return JSON.parse(raw) as Template[];
  } catch {
    // 存储不可用 → 回落内置默认
  }
  return DEFAULTS;
}

export function saveTemplates(items: Template[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(items));
  } catch {
    // 静默忽略
  }
}

export function newTemplateId(): string {
  return "t" + Math.random().toString(36).slice(2, 9);
}
