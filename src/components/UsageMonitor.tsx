import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';

interface UsageStats {
  request_count: number;
  input_tokens: number;
  output_tokens: number;
}

interface ModelUsageStats {
  model: string;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface DailyUsageStats {
  date: string;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface UsageLog {
  id: number;
  timestamp: number;
  account_id: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  session_id?: string;
}

interface SessionUsageStats {
  session_id: string;
  first_request: number;
  last_request: number;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

function Tooltip({ text, children }: { text: string; children: React.ReactNode }) {
  return (
    <span className="group relative inline-flex items-center">
      {children}
      <span className="invisible group-hover:visible absolute left-1/2 -translate-x-1/2 top-full mt-1 px-2 py-1 bg-gray-800 text-white text-xs rounded whitespace-nowrap z-10">
        {text}
        <span className="absolute bottom-full left-1/2 -translate-x-1/2 border-4 border-transparent border-b-gray-800"></span>
      </span>
    </span>
  );
}

// Simple Bar Chart Component
function BarChart({ data, height = 200 }: { data: DailyUsageStats[]; height?: number }) {
  const { t } = useTranslation();

  if (data.length === 0) return null;

  const maxTokens = Math.max(...data.map(d => d.total_input_tokens + d.total_output_tokens), 1);
  const padding = 40;
  const chartHeight = height - padding;

  const formatK = (num: number) => {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(0)}K`;
    return num.toString();
  };

  return (
    <div className="w-full">
      <svg width="100%" height={height} className="overflow-visible">
        {/* Y-axis labels */}
        <text x="0" y="15" className="text-xs fill-gray-400">{formatK(maxTokens)}</text>
        <text x="0" y={chartHeight / 2 + 5} className="text-xs fill-gray-400">{formatK(maxTokens / 2)}</text>
        <text x="0" y={chartHeight} className="text-xs fill-gray-400">0</text>

        {/* Grid lines */}
        <line x1={padding} y1="10" x2="100%" y2="10" stroke="#e5e7eb" strokeDasharray="4" />
        <line x1={padding} y1={chartHeight / 2} x2="100%" y2={chartHeight / 2} stroke="#e5e7eb" strokeDasharray="4" />
        <line x1={padding} y1={chartHeight - 5} x2="100%" y2={chartHeight - 5} stroke="#e5e7eb" />

        {/* Bars */}
        {data.map((day, idx) => {
          const inputHeight = (day.total_input_tokens / maxTokens) * (chartHeight - 20);
          const outputHeight = (day.total_output_tokens / maxTokens) * (chartHeight - 20);
          const totalHeight = inputHeight + outputHeight;
          const x = padding + (idx * (100 - padding) / data.length) + '%';
          const barW = `${(100 - padding) / data.length - 2}%`;

          return (
            <g key={idx}>
              {/* Input tokens bar (green) */}
              <rect
                x={x}
                y={chartHeight - 5 - totalHeight}
                width={barW}
                height={inputHeight}
                fill="#22c55e"
                rx="2"
                className="cursor-pointer hover:opacity-80"
              >
                <title>{t('usage.input')}: {day.total_input_tokens.toLocaleString()}</title>
              </rect>
              {/* Output tokens bar (purple) */}
              <rect
                x={x}
                y={chartHeight - 5 - outputHeight}
                width={barW}
                height={outputHeight}
                fill="#a855f7"
                rx="2"
                className="cursor-pointer hover:opacity-80"
              >
                <title>{t('usage.output')}: {day.total_output_tokens.toLocaleString()}</title>
              </rect>
              {/* Date label */}
              <text
                x={`calc(${x} + ${parseFloat(barW) / 2}%)`}
                y={chartHeight + 15}
                textAnchor="middle"
                className="text-xs fill-gray-500"
              >
                {day.date.slice(5)}
              </text>
            </g>
          );
        })}
      </svg>

      {/* Legend */}
      <div className="flex justify-center gap-4 mt-2 text-xs">
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 bg-green-500 rounded"></div>
          <span className="text-gray-600">{t('usage.input')}</span>
        </div>
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 bg-purple-500 rounded"></div>
          <span className="text-gray-600">{t('usage.output')}</span>
        </div>
      </div>
    </div>
  );
}

// Model Distribution Pie Chart
function ModelPieChart({ data }: { data: ModelUsageStats[] }) {
  if (data.length === 0) return null;

  const total = data.reduce((sum, m) => sum + m.total_input_tokens + m.total_output_tokens, 0);
  const colors = ['#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#8b5cf6', '#06b6d4'];

  let currentAngle = 0;

  const getPath = (percentage: number) => {
    const startAngle = currentAngle;
    const angle = (percentage / 100) * 360;
    currentAngle += angle;
    const endAngle = currentAngle;

    const startRad = (startAngle - 90) * (Math.PI / 180);
    const endRad = (endAngle - 90) * (Math.PI / 180);

    const x1 = 50 + 40 * Math.cos(startRad);
    const y1 = 50 + 40 * Math.sin(startRad);
    const x2 = 50 + 40 * Math.cos(endRad);
    const y2 = 50 + 40 * Math.sin(endRad);

    const largeArc = angle > 180 ? 1 : 0;

    return `M 50 50 L ${x1} ${y1} A 40 40 0 ${largeArc} 1 ${x2} ${y2} Z`;
  };

  return (
    <div className="flex items-center gap-4">
      <svg width="120" height="120" viewBox="0 0 100 100">
        {data.slice(0, 6).map((model, idx) => {
          const tokens = model.total_input_tokens + model.total_output_tokens;
          const percentage = (tokens / total) * 100;
          if (percentage < 1) return null;
          return (
            <path
              key={idx}
              d={getPath(percentage)}
              fill={colors[idx % colors.length]}
              className="hover:opacity-80 cursor-pointer"
            >
              <title>{model.model}: {percentage.toFixed(1)}%</title>
            </path>
          );
        })}
      </svg>
      <div className="flex-1 space-y-1">
        {data.slice(0, 6).map((model, idx) => {
          const tokens = model.total_input_tokens + model.total_output_tokens;
          const percentage = (tokens / total) * 100;
          return (
            <div key={idx} className="flex items-center gap-2 text-xs">
              <div
                className="w-2 h-2 rounded-full"
                style={{ backgroundColor: colors[idx % colors.length] }}
              ></div>
              <span className="truncate flex-1 text-gray-700">{model.model.split('-').slice(-2).join('-')}</span>
              <span className="text-gray-500">{percentage.toFixed(0)}%</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default function UsageMonitor() {
  const { t } = useTranslation();
  const [stats, setStats] = useState<UsageStats>({
    request_count: 0,
    input_tokens: 0,
    output_tokens: 0,
  });
  const [modelStats, setModelStats] = useState<ModelUsageStats[]>([]);
  const [dailyStats, setDailyStats] = useState<DailyUsageStats[]>([]);
  const [sessionStats, setSessionStats] = useState<SessionUsageStats[]>([]);
  const [recentLogs, setRecentLogs] = useState<UsageLog[]>([]);
  const [activeTab, setActiveTab] = useState<'overview' | 'models' | 'daily' | 'sessions' | 'logs'>('overview');
  const [viewMode, setViewMode] = useState<'list' | 'chart'>('list');

  useEffect(() => {
    loadStats();

    // 10초마다 업데이트 (사용량 통계는 실시간 필요 없음, OOM 방지)
    const interval = setInterval(() => {
      loadStats();
    }, 10000);

    return () => clearInterval(interval);
  }, []);

  const loadStats = async () => {
    try {
      const [statsResult, modelResult, dailyResult, sessionResult, logsResult] = await Promise.all([
        invoke('get_usage_stats'),
        invoke('get_usage_by_model'),
        invoke('get_daily_usage', { days: 7 }),
        invoke('get_usage_by_session'),
        invoke('get_recent_usage', { limit: 10 }),
      ]);
      setStats(statsResult as UsageStats);
      setModelStats(modelResult as ModelUsageStats[]);
      setDailyStats(dailyResult as DailyUsageStats[]);
      setSessionStats(sessionResult as SessionUsageStats[]);
      setRecentLogs(logsResult as UsageLog[]);
    } catch (error) {
      console.error('Failed to load usage stats:', error);
    }
  };

  const formatNumber = (num: number) => {
    return new Intl.NumberFormat().format(num);
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  const tabs = [
    { id: 'overview', label: t('usage.tabs.overview') },
    { id: 'models', label: t('usage.tabs.models') },
    { id: 'daily', label: t('usage.tabs.daily') },
    { id: 'sessions', label: t('usage.tabs.sessions') },
    { id: 'logs', label: t('usage.tabs.logs') },
  ] as const;

  // Show toggle for tabs that have chart view
  const showViewToggle = activeTab === 'daily' || activeTab === 'models';

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-lg font-semibold text-gray-900">{t('usage.title')}</h2>

        {/* View Toggle */}
        {showViewToggle && (
          <div className="flex bg-gray-100 rounded-lg p-0.5">
            <button
              onClick={() => setViewMode('list')}
              className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
                viewMode === 'list'
                  ? 'bg-white text-gray-900 shadow'
                  : 'text-gray-500 hover:text-gray-700'
              }`}
            >
              List
            </button>
            <button
              onClick={() => setViewMode('chart')}
              className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
                viewMode === 'chart'
                  ? 'bg-white text-gray-900 shadow'
                  : 'text-gray-500 hover:text-gray-700'
              }`}
            >
              Chart
            </button>
          </div>
        )}
      </div>

      {/* 탭 네비게이션 */}
      <div className="flex space-x-1 mb-4 bg-gray-100 rounded-lg p-1">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 px-3 py-2 text-sm font-medium rounded-md transition-colors ${
              activeTab === tab.id
                ? 'bg-white text-gray-900 shadow'
                : 'text-gray-600 hover:text-gray-900'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* 개요 탭 */}
      {activeTab === 'overview' && (
        <div className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="bg-blue-50 rounded-lg p-4">
              <Tooltip text={t('usage.requestCountTooltip')}>
                <div className="text-sm text-blue-600 font-medium mb-1 cursor-help">
                  {t('usage.requestCount')} ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.request_count)}</div>
            </div>

            <div className="bg-green-50 rounded-lg p-4">
              <Tooltip text={t('usage.inputTokensTooltip')}>
                <div className="text-sm text-green-600 font-medium mb-1 cursor-help">
                  {t('usage.inputTokens')} ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.input_tokens)}</div>
            </div>

            <div className="bg-purple-50 rounded-lg p-4">
              <Tooltip text={t('usage.outputTokensTooltip')}>
                <div className="text-sm text-purple-600 font-medium mb-1 cursor-help">
                  {t('usage.outputTokens')} ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.output_tokens)}</div>
            </div>
          </div>

          <div className="bg-gray-50 rounded-lg p-4">
            <Tooltip text={t('usage.totalTokensTooltip')}>
              <span className="text-sm text-gray-600 cursor-help">
                {t('usage.totalTokens')} ⓘ
              </span>
            </Tooltip>
            <span className="text-sm text-gray-600">: <span className="font-semibold">{formatNumber(stats.input_tokens + stats.output_tokens)}</span></span>
          </div>
        </div>
      )}

      {/* 모델별 탭 */}
      {activeTab === 'models' && (
        <div className="space-y-2">
          {modelStats.length === 0 ? (
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : viewMode === 'chart' ? (
            <div className="p-4">
              <ModelPieChart data={modelStats} />
            </div>
          ) : (
            modelStats.map((model, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-medium text-gray-900">{model.model || 'unknown'}</div>
                    <div className="text-sm text-gray-500">{model.request_count}{t('usage.requests')}</div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm text-gray-600">
                      {t('usage.input')}: {formatNumber(model.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      {t('usage.output')}: {formatNumber(model.total_output_tokens)}
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 일별 탭 */}
      {activeTab === 'daily' && (
        <div className="space-y-2">
          {dailyStats.length === 0 ? (
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : viewMode === 'chart' ? (
            <div className="p-4">
              <BarChart data={dailyStats} height={220} />
            </div>
          ) : (
            dailyStats.map((day, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-medium text-gray-900">{day.date}</div>
                    <div className="text-sm text-gray-500">{day.request_count}{t('usage.requests')}</div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm text-gray-600">
                      {t('usage.input')}: {formatNumber(day.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      {t('usage.output')}: {formatNumber(day.total_output_tokens)}
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 세션별 탭 */}
      {activeTab === 'sessions' && (
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {sessionStats.length === 0 ? (
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : (
            sessionStats.map((session, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-start">
                  <div className="flex-1 min-w-0">
                    <Tooltip text={`${t('usage.sessionId')}: ${session.session_id}`}>
                      <div className="font-mono text-xs text-gray-500 truncate cursor-help">
                        {session.session_id.substring(0, 12)}...
                      </div>
                    </Tooltip>
                    <div className="text-sm text-gray-600 mt-1">
                      {formatDate(session.first_request)} ~ {formatDate(session.last_request)}
                    </div>
                    <div className="text-sm text-gray-500">{session.request_count}{t('usage.requests')}</div>
                  </div>
                  <div className="text-right ml-4">
                    <div className="text-sm text-gray-600">
                      {t('usage.input')}: {formatNumber(session.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      {t('usage.output')}: {formatNumber(session.total_output_tokens)}
                    </div>
                    <div className="text-xs text-gray-400 mt-1">
                      {t('usage.total')} {formatNumber(session.total_input_tokens + session.total_output_tokens)} {t('usage.tokens')}
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 최근 로그 탭 */}
      {activeTab === 'logs' && (
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {recentLogs.length === 0 ? (
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : (
            recentLogs.map((log) => (
              <div key={log.id} className="bg-gray-50 rounded-lg p-3 text-sm">
                <div className="flex justify-between">
                  <span className="font-medium">{log.model || 'unknown'}</span>
                  <span className="text-gray-500">{formatDate(log.timestamp)}</span>
                </div>
                <div className="text-gray-600 mt-1">
                  {t('usage.input')}: {formatNumber(log.input_tokens)} / {t('usage.output')}: {formatNumber(log.output_tokens)}
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
