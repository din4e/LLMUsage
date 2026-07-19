import { formatInteger, type DailyTrendPoint } from "./domain";

const svgElement = <K extends keyof SVGElementTagNameMap>(name: K): SVGElementTagNameMap[K] =>
  document.createElementNS("http://www.w3.org/2000/svg", name);

export function renderDailyTrendChart(
  chart: SVGSVGElement,
  empty: HTMLElement | null,
  descriptionElement: HTMLElement | null,
  points: DailyTrendPoint[],
  selectedName: string,
  intraday: boolean,
) {
  chart.replaceChildren();
  if (empty) empty.hidden = points.length > 0;
  if (!points.length) {
    if (descriptionElement) descriptionElement.textContent = "本地保存的非敏感日汇总 · 暂无历史数据";
    chart.setAttribute("aria-label", "所选范围暂无每日用量数据");
    return;
  }

  const width = 900;
  const height = 250;
  const plot = { left: 58, right: 18, top: 18, bottom: 34 };
  const plotWidth = width - plot.left - plot.right;
  const plotHeight = height - plot.top - plot.bottom;
  const maxTokens = Math.max(...points.map((point) => point.totalTokens ?? 0), 1);
  const x = (index: number) => plot.left + (points.length === 1
    ? plotWidth / 2
    : (index / (points.length - 1)) * plotWidth);
  const y = (value: number) => plot.top + plotHeight - (value / maxTokens) * plotHeight;

  for (let tick = 0; tick <= 4; tick += 1) {
    const value = (maxTokens * tick) / 4;
    const tickY = y(value);
    const line = svgElement("line");
    line.setAttribute("class", "trend-grid-line");
    line.setAttribute("x1", String(plot.left));
    line.setAttribute("x2", String(width - plot.right));
    line.setAttribute("y1", String(tickY));
    line.setAttribute("y2", String(tickY));
    const label = svgElement("text");
    label.setAttribute("class", "trend-axis-label");
    label.setAttribute("x", String(plot.left - 9));
    label.setAttribute("y", String(tickY + 4));
    label.setAttribute("text-anchor", "end");
    label.textContent = formatInteger(value);
    chart.append(line, label);
  }

  const path = svgElement("path");
  path.setAttribute("class", "trend-line");
  path.setAttribute("d", points
    .map((point, index) => `${index ? "L" : "M"}${x(index)},${y(point.totalTokens ?? 0)}`)
    .join(" "));
  chart.append(path);

  const labelIndexes = new Set([0, Math.floor((points.length - 1) / 2), points.length - 1]);
  points.forEach((point, index) => {
    const circle = svgElement("circle");
    circle.setAttribute("class", "trend-point");
    circle.setAttribute("cx", String(x(index)));
    circle.setAttribute("cy", String(y(point.totalTokens ?? 0)));
    circle.setAttribute("r", points.length > 48 ? "2" : "4");
    circle.setAttribute("tabindex", "0");
    const cost = point.estimatedCostCny == null
      ? "成本不可用"
      : `成本 ¥${point.estimatedCostCny.toFixed(2)}`;
    const requests = point.requests == null ? "请求数不可用" : `${formatInteger(point.requests)} 次请求`;
    const pointDescription = `${point.label}，${formatInteger(point.totalTokens ?? 0)} Token，${requests}，${cost}`;
    circle.setAttribute("aria-label", pointDescription);
    const title = svgElement("title");
    title.textContent = pointDescription;
    circle.append(title);
    chart.append(circle);

    if (labelIndexes.has(index)) {
      const label = svgElement("text");
      label.setAttribute("class", "trend-axis-label trend-date-label");
      label.setAttribute("x", String(x(index)));
      label.setAttribute("y", String(height - 9));
      label.setAttribute("text-anchor", index === 0 ? "start" : index === points.length - 1 ? "end" : "middle");
      label.textContent = point.label;
      chart.append(label);
    }
  });

  const totalTokens = intraday
    ? points[points.length - 1]?.totalTokens ?? 0
    : points.reduce((total, point) => total + (point.totalTokens ?? 0), 0);
  const totalLabel = intraday ? "今日累计" : "合计";
  const description = `${selectedName} · ${points.length} 个数据点 · ${totalLabel} ${formatInteger(totalTokens)} Token`;
  chart.setAttribute("aria-label", description);
  if (descriptionElement) descriptionElement.textContent = `本地保存的非敏感日汇总 · ${description}`;
}
