// Static help URLs contain no business identifiers or credentials.
export const pageHelp: Record<string, string> = {
  posts: 'operator/moderation',
  comments: 'operator/moderation',
  cases: 'operator/moderation',
  resources: 'operator/moderation#display-resources',
  groups: 'operator/groups-and-templates',
  templates: 'operator/groups-and-templates',
  'broadcast-tasks': 'operator/broadcast-tasks',
  'mobile-delivery': 'operator/mobile-tasks',
  members: 'school-admin/settings-and-members',
  settings: 'school-admin/settings-and-members',
  audit: 'platform-admin/audit-and-resources#audit',
  schools: 'platform-admin/accounts-and-schools',
  accounts: 'platform-admin/accounts-and-schools',
  administrators: 'platform-admin/accounts-and-schools',
  'wecom-assets': 'platform-admin/directory-and-assets',
  'wecom-directory': 'platform-admin/directory-and-assets',
  'execution-devices': 'operator/execution-resources',
  'execution-accounts': 'operator/execution-resources#sending-accounts',
  'platform-resources': 'platform-admin/audit-and-resources',
}
export const helpHref = (active: string) =>
  pageHelp[active] ? `/docs/guide/${pageHelp[active]}` : '/docs/guide/'
