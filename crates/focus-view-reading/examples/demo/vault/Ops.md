# Ops

Commands of the engagement, with context variables.

```bash
nmap -sC -sV -p- $RHOST -oA scans/$RHOST
evil-winrm -i $RHOST -u $USER -p $PASSWORD
nc -lvnp $LPORT
```
