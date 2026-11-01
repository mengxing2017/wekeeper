import { SharedSendTasks } from './shared-send-tasks'

type Props = { school: string; admin: boolean; onNavigate?: (view: string) => void }
export function MobileExecutionTasks(props: Props) {
  return <SharedSendTasks key={`${props.school}:${props.admin}`} {...props} />
}
