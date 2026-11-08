// Static help URLs contain no business identifiers or credentials.
export const pageHelp: Record<string, string> = {
  'source-credentials': 'operator/source-access',

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
