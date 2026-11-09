// Static help URLs contain no business identifiers or credentials.
export const pageHelp: Record<string, string> = {
  'source-credentials': 'operator/source-access',
  'source-routing-directory': 'operator/source-access',
  advertisements: 'operator/source-access',
  'advertisement-rotations': 'operator/source-access',
  'keyword-rules': 'operator/source-access',
  'source-automation-tasks': 'operator/source-access',
  'source-receipts': 'operator/source-access',
  'source-route-configurations': 'operator/source-access',

  'wecom-workspaces': 'workspaces',
  groups: 'operator/groups-and-templates',
  templates: 'operator/groups-and-templates',
  'broadcast-tasks': 'operator/broadcast-tasks',
  audit: 'platform-admin/audit-and-resources#audit',
  administrators: 'platform-admin/administrators',
  'wecom-assets': 'platform-admin/directory-and-assets',
  'wecom-directory': 'platform-admin/directory-and-assets',
  'mobile-execution-tasks': 'operator/mobile-execution-tasks',
  'mobile-targets': 'operator/mobile-targets',
  'shared-mobile-resources': 'platform-admin/shared-mobile-resources',
  'execution-devices': 'operator/execution-resources',
  'wecom-senders': 'operator/official-senders',
}
export const helpHref = (active: string) =>
  pageHelp[active] ? `/docs/guide/${pageHelp[active]}` : '/docs/guide/'
