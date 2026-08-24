import {
  formatInteger,
  formatProviderChangeValue,
  formatQuarterSlot,
  type BalanceTrendPoint,
  type DailyTrendPoint,
  type ProviderChangeMetric,
  type ProviderChangePoint,
} from "./domain";

/** Compact ¥ label for the balance axis: ¥1234.5 → ¥1.2k. */
function formatCurrencyAxis(value: number): string {
  if (value >= 1000) return `¥${(value / 1000).toFixed(value >= 10_000 ? 0 : 1)}k`;
  return `¥${Math.round(value)}`;
}

function formatCurrencyExact(value: number): string {
  return `¥${value.toFixed(2)}`;
}

const svgElement = <K extends keyof SVGElementTagNameMap>(name: K): SVGElementTagNameMap[K] =>
  document.createElementNS("http://www.w3.org/2000/svg", name);

const providerChangeMetricLabels: Record<ProviderChangeMetric, string> = {
  requests: "请求",
  tokens: "Token",
  balance: "余额",
  cost: "成本",
};

function providerChangePointLabel(point: ProviderChangePoint): string {
  const date = point.date.slice(5).replace("-", "/");
  return point.slot == null ? date : `${date} ${formatQuarterSlot(point.slot)}`;
}

export function renderProviderChangeChart(
  chart: SVGSVGElement,
  points: ProviderChangePoint[],
  metric: ProviderChangeMetric,
  selectedName: string,
) {
  chart.replaceChildren();
  if (points.length < 2) {
    chart.setAttribute("aria-label", `${selectedName} 的${providerChangeMetricLabels[metric]}曲线至少需要两个采样点`);
    return;
  }

  const width = 600;
  const height = 98;
  const plot = { left: 12, right: 12, top: 10, bottom: 22 };
  const plotWidth = width - plot.left - plot.right;
  const plotHeight = height - plot.top - plot.bottom;
  const values = points.map((point) => point.value);
  const minValue = Math.min(...values);
  const maxValue = Math.max(...values);
  const spread = maxValue - minValue;
  const x = (index: number) => plot.left + (index / (points.length - 1)) * plotWidth;
  const y = (value: number) => spread === 0
    ? plot.top + plotHeight / 2
    : plot.top + ((maxValue - value) / spread) * plotHeight;

  for (const tickY of [plot.top, plot.top + plotHeight / 2, plot.top + plotHeight]) {
    const grid = svgElement("line");
    grid.setAttribute("class", "recent-change-chart-grid");
    grid.setAttribute("x1", String(plot.left));
    grid.setAttribute("x2", String(width - plot.right));
    grid.setAttribute("y1", String(tickY));
    grid.setAttribute("y2", String(tickY));
    chart.append(grid);
  }

  const coords = points.map((point, index) => [x(index), y(point.value)] as const);
  const path = svgElement("path");
  path.setAttribute("class", "recent-change-chart-line");
  path.setAttribute("d", coords.map(([pointX, pointY], index) => `${index ? "L" : "M"}${pointX},${pointY}`).join(" "));
  chart.append(path);

  const lastIndex = points.length - 1;
  points.forEach((point, index) => {
    const [pointX, pointY] = coords[index]!;
    const description = `${selectedName}，${providerChangePointLabel(point)}，${providerChangeMetricLabels[metric]} ${formatProviderChangeValue(metric, point.value, false)}`;
    const circle = svgElement("circle");
    circle.setAttribute("class", "recent-change-chart-point");
    if (index === lastIndex) circle.classList.add("latest");
    circle.setAttribute("cx", String(pointX));
    circle.setAttribute("cy", String(pointY));
    circle.setAttribute("r", index === lastIndex ? "4" : "3");
    circle.setAttribute("tabindex", "0");
    circle.setAttribute("aria-label", description);
    const title = svgElement("title");
    title.textContent = description;
    circle.append(title);
    chart.append(circle);
  });

  for (const index of new Set([0, Math.floor(lastIndex / 2), lastIndex])) {
    const label = svgElement("text");
    label.setAttribute("class", "recent-change-chart-label");
    label.setAttribute("x", String(coords[index]![0]));
    label.setAttribute("y", String(height - 5));
    label.setAttribute("text-anchor", index === 0 ? "start" : index === lastIndex ? "end" : "middle");
    label.textContent = providerChangePointLabel(points[index]!);
    chart.append(label);
  }

  chart.setAttribute(
    "aria-label",
    `${selectedName}最近${providerChangeMetricLabels[metric]}曲线，共 ${points.length} 个采样点`,
  );
}

/** Catmull-Rom spline converted to cubic beziers, clamped to the plot box so
 *  smoothing never draws outside the value range. */
function smoothLinePath(
  coords: ReadonlyArray<readonly [number, number]>,
  top: number,
  bottom: number,
): string {
  if (coords.length < 3) {
    return coords.map(([px, py], index) => `${index ? "L" : "M"}${px},${py}`).join(" ");
  }
  const clampY = (value: number) => Math.min(Math.max(value, top), bottom);
  let path = `M${coords[0]![0]},${coords[0]![1]}`;
  for (let index = 0; index < coords.length - 1; index += 1) {
    const previous = coords[index - 1] ?? coords[index]!;
    const current = coords[index]!;
    const next = coords[index + 1]!;
    const after = coords[index + 2] ?? next;
    const firstControlX = current[0] + (next[0] - previous[0]) / 6;
    const firstControlY = clampY(current[1] + (next[1] - previous[1]) / 6);
    const secondControlX = next[0] - (after[0] - current[0]) / 6;
    const secondControlY = clampY(next[1] - (after[1] - current[1]) / 6);
    path += ` C${firstControlX},${firstControlY} ${secondControlX},${secondControlY} ${next[0]},${next[1]}`;
  }
  return path;
}

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
    // The balance renderer swaps this copy; restore the token wording on switch.
    if (empty) empty.textContent = "同步供应商后，将从当天开始积累每日趋势。";
    if (descriptionElement) descriptionElement.textContent = "本地保存的非敏感日汇总 · 暂无历史数据";
    chart.setAttribute("aria-label", "所选范围暂无每日用量数据");
    return;
  }

  const width = 900;
  const height = 190;
  const plot = { left: 48, right: 16, top: 12, bottom: 26 };
  const plotWidth = width - plot.left - plot.right;
  const plotHeight = height - plot.top - plot.bottom;
  const baseY = plot.top + plotHeight;
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

  const coords = points.map((point, index) => [x(index), y(point.totalTokens ?? 0)] as const);
  const linePath = smoothLinePath(coords, plot.top, baseY);

  // Soft gradient fill under the curve anchors the eye on the trend shape.
  const defs = svgElement("defs");
  const gradient = svgElement("linearGradient");
  gradient.setAttribute("id", "trend-area-fill");
  gradient.setAttribute("x1", "0");
  gradient.setAttribute("y1", "0");
  gradient.setAttribute("x2", "0");
  gradient.setAttribute("y2", "1");
  const topStop = svgElement("stop");
  topStop.setAttribute("offset", "0");
  topStop.style.stopColor = "var(--accent)";
  topStop.setAttribute("stop-opacity", "0.22");
  const bottomStop = svgElement("stop");
  bottomStop.setAttribute("offset", "1");
  bottomStop.style.stopColor = "var(--accent)";
  bottomStop.setAttribute("stop-opacity", "0");
  gradient.append(topStop, bottomStop);
  defs.append(gradient);
  chart.append(defs);

  if (coords.length > 1) {
    const area = svgElement("path");
    area.setAttribute("class", "trend-area");
    area.setAttribute(
      "d",
      `${linePath} L${coords[coords.length - 1]![0]},${baseY} L${coords[0]![0]},${baseY} Z`,
    );
    chart.append(area);
  }

  const path = svgElement("path");
  path.setAttribute("class", "trend-line");
  path.setAttribute("d", linePath);
  chart.append(path);

  const lastIndex = points.length - 1;
  const labelIndexes = new Set([0, Math.floor(lastIndex / 2), lastIndex]);
  points.forEach((point, index) => {
    const [pointX, pointY] = coords[index]!;
    const circle = svgElement("circle");
    circle.setAttribute("class", "trend-point");
    if (index === lastIndex) circle.classList.add("latest");
    circle.setAttribute("cx", String(pointX));
    circle.setAttribute("cy", String(pointY));
    circle.setAttribute(
      "r",
      index === lastIndex ? (points.length > 48 ? "2.6" : "5") : points.length > 48 ? "2" : "3.5",
    );
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
      label.setAttribute("x", String(pointX));
      label.setAttribute("y", String(height - 8));
      label.setAttribute("text-anchor", index === 0 ? "start" : index === lastIndex ? "end" : "middle");
      label.textContent = point.label;
      chart.append(label);
    }
  });

  // Latest value rides next to the last point so the current figure is readable
  // without hovering.
  const lastPoint = points[lastIndex]!;
  const lastCoord = coords[lastIndex]!;
  const latestLabel = svgElement("text");
  latestLabel.setAttribute("class", "trend-axis-label trend-value-label");
  latestLabel.setAttribute("x", String(lastCoord[0] - 9));
  latestLabel.setAttribute(
    "y",
    String(lastCoord[1] - 9 < plot.top + 12 ? lastCoord[1] + 18 : lastCoord[1] - 9),
  );
  latestLabel.setAttribute("text-anchor", "end");
  latestLabel.textContent = formatInteger(lastPoint.totalTokens ?? 0);
  chart.append(latestLabel);

  const totalTokens = intraday
    ? points[points.length - 1]?.totalTokens ?? 0
    : points.reduce((total, point) => total + (point.totalTokens ?? 0), 0);
  const totalLabel = intraday ? "今日累计" : "合计";
  const description = `${selectedName} · ${points.length} 个数据点 · ${totalLabel} ${formatInteger(totalTokens)} Token`;
  chart.setAttribute("aria-label", description);
  if (descriptionElement) descriptionElement.textContent = `本地保存的非敏感日汇总 · ${description}`;
}

/** Balance curve: same skeleton as the token chart but ¥-scaled, 0-based so
 *  depletion reads honestly, and the latest total rides the last point. */
export function renderBalanceTrendChart(
  chart: SVGSVGElement,
  empty: HTMLElement | null,
  descriptionElement: HTMLElement | null,
  points: BalanceTrendPoint[],
  selectedName: string,
  intraday: boolean,
) {
  chart.replaceChildren();
  if (empty) empty.hidden = points.length > 0;
  if (!points.length) {
    if (empty) empty.textContent = "同步含余额的供应商后，将随同步积累余额变化曲线。";
    if (descriptionElement) descriptionElement.textContent = "本地保存的非敏感日汇总 · 暂无余额数据";
    chart.setAttribute("aria-label", "所选范围暂无余额数据");
    return;
  }

  const width = 900;
  const height = 190;
  const plot = { left: 48, right: 16, top: 12, bottom: 26 };
  const plotWidth = width - plot.left - plot.right;
  const plotHeight = height - plot.top - plot.bottom;
  const baseY = plot.top + plotHeight;
  const maxBalance = Math.max(...points.map((point) => point.balanceCny), 1);
  const x = (index: number) => plot.left + (points.length === 1
    ? plotWidth / 2
    : (index / (points.length - 1)) * plotWidth);
  const y = (value: number) => plot.top + plotHeight - (value / maxBalance) * plotHeight;

  for (let tick = 0; tick <= 4; tick += 1) {
    const value = (maxBalance * tick) / 4;
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
    label.textContent = formatCurrencyAxis(value);
    chart.append(line, label);
  }

  const coords = points.map((point, index) => [x(index), y(point.balanceCny)] as const);
  const linePath = smoothLinePath(coords, plot.top, baseY);

  const defs = svgElement("defs");
  const gradient = svgElement("linearGradient");
  gradient.setAttribute("id", "trend-balance-fill");
  gradient.setAttribute("x1", "0");
  gradient.setAttribute("y1", "0");
  gradient.setAttribute("x2", "0");
  gradient.setAttribute("y2", "1");
  const topStop = svgElement("stop");
  topStop.setAttribute("offset", "0");
  topStop.style.stopColor = "var(--accent)";
  topStop.setAttribute("stop-opacity", "0.22");
  const bottomStop = svgElement("stop");
  bottomStop.setAttribute("offset", "1");
  bottomStop.style.stopColor = "var(--accent)";
  bottomStop.setAttribute("stop-opacity", "0");
  gradient.append(topStop, bottomStop);
  defs.append(gradient);
  chart.append(defs);

  if (coords.length > 1) {
    const area = svgElement("path");
    area.setAttribute("class", "trend-area");
    area.setAttribute(
      "d",
      `${linePath} L${coords[coords.length - 1]![0]},${baseY} L${coords[0]![0]},${baseY} Z`,
    );
    chart.append(area);
  }

  const path = svgElement("path");
  path.setAttribute("class", "trend-line");
  path.setAttribute("d", linePath);
  chart.append(path);

  const lastIndex = points.length - 1;
  const labelIndexes = new Set([0, Math.floor(lastIndex / 2), lastIndex]);
  points.forEach((point, index) => {
    const [pointX, pointY] = coords[index]!;
    const circle = svgElement("circle");
    circle.setAttribute("class", "trend-point");
    if (index === lastIndex) circle.classList.add("latest");
    circle.setAttribute("cx", String(pointX));
    circle.setAttribute("cy", String(pointY));
    circle.setAttribute(
      "r",
      index === lastIndex ? (points.length > 48 ? "2.6" : "5") : points.length > 48 ? "2" : "3.5",
    );
    circle.setAttribute("tabindex", "0");
    const providerScope = point.providers > 1 ? ` · ${point.providers} 个实例` : "";
    const pointDescription = `${point.label}，余额 ${formatCurrencyExact(point.balanceCny)}${providerScope}`;
    circle.setAttribute("aria-label", pointDescription);
    const title = svgElement("title");
    title.textContent = pointDescription;
    circle.append(title);
    chart.append(circle);

    if (labelIndexes.has(index)) {
      const label = svgElement("text");
      label.setAttribute("class", "trend-axis-label trend-date-label");
      label.setAttribute("x", String(pointX));
      label.setAttribute("y", String(height - 8));
      label.setAttribute("text-anchor", index === 0 ? "start" : index === lastIndex ? "end" : "middle");
      label.textContent = point.label;
      chart.append(label);
    }
  });

  const lastPoint = points[lastIndex]!;
  const lastCoord = coords[lastIndex]!;
  const latestLabel = svgElement("text");
  latestLabel.setAttribute("class", "trend-axis-label trend-value-label");
  latestLabel.setAttribute("x", String(lastCoord[0] - 9));
  latestLabel.setAttribute(
    "y",
    String(lastCoord[1] - 9 < plot.top + 12 ? lastCoord[1] + 18 : lastCoord[1] - 9),
  );
  latestLabel.setAttribute("text-anchor", "end");
  latestLabel.textContent = formatCurrencyExact(lastPoint.balanceCny);
  chart.append(latestLabel);

  const scope = intraday ? "当前" : "最新";
  const description = `${selectedName} · ${points.length} 个数据点 · ${scope}余额 ${formatCurrencyExact(lastPoint.balanceCny)}`;
  chart.setAttribute("aria-label", description);
  if (descriptionElement) descriptionElement.textContent = `本地保存的非敏感日汇总 · ${description}`;
}
