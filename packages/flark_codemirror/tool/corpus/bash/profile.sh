# Shell helpers sourced from ~/.profile; kept POSIX-friendly where it matters.

export EDITOR="vim" PAGER='less -R'
export PATH="$HOME/.local/bin:/opt/tools/bin${PATH:+:$PATH}"
umask 022

alias ll='ls -lah'
alias gs="git status --short --branch"

# Jump to a project by prefix: `p web` opens ~/work/website.
p() {
  local match
  match=$(ls -d ~/work/"$1"* 2>/dev/null | head -n 1)
  if [ -z "$match" ]; then
    echo "no project matching '$1'" >&2
    return 1
  elif [ -d "$match/.git" ]; then
    cd "$match" && git fetch --quiet &
  else
    cd "$match" || return
  fi
}

# Extract any archive by extension.
extract() {
  for f in "$@"; do
    case $f in
      *.tar.gz|*.tgz) tar xzf "$f" ;;
      *.tar.bz2) tar xjf "$f" ;;
      *.zip) unzip -q "$f" ;;
      *.gz) gunzip "$f" ;;
      *) echo "extract: don't know how to handle $f"; return 1 ;;
    esac
  done
}

mkcd() { mkdir -p -- "$1" && cd -P -- "$1"; }

weather() {
  local city=${1// /+}
  curl -fsS "https://wttr.example.test/${city:-Toronto}?format=3" \
    || echo "offline"
}

# Count lines of code per extension in the current tree.
loc() {
  find . -type f -name '*.*' ! -path './.git/*' \
    | sed 's/.*\.//' | sort | uniq -c | sort -rn | head -${1:-10}
}

retry() {
  local n=0 max=${RETRIES:-5}
  until "$@"; do
    n=$((n+1))
    [ "$n" -ge "$max" ] && return 1
    sleep "$n"
  done
}

nums=(1 2 3 5 8 13)
total=0
for ((i = 0; i < ${#nums[@]}; i++)); do
  total=$(( total + nums[i] ))
done
echo "sum of ${nums[*]} is $total, last index $((${#nums[@]} - 1))"

greeting=$'tab:\tnewline:\n'
printf "%s" "$greeting"
name="world"
echo "hello, ${name^}! You have $(( 3 * 7 )) new messages."
echo 'single quotes keep $name and $(this) literal'
echo "nested $(echo "inner $(date +%Y) done") outer"
echo "backticks: `whoami` and `echo "quoted inside"`"
echo "escaped \"quotes\" and a trailing dollar $"

if command -v brew >/dev/null 2>&1; then
  eval "$(brew shellenv)"
fi

[[ -f ~/.secrets ]] && source ~/.secrets
[[ $- == *i* ]] && PS1='\u@\h:\w\$ '

python3 - "$@" <<"PY"
import sys
print(sys.argv[1:])
PY
wc -l <<< "one line via here-string"
read -r first rest <<< "$greeting"
while read -r line; do echo "${#line}: $line"; done < /etc/hosts
exec 3>&- 2>>"$HOME/.cache/profile.err"

# A <<- body ends at a tab-indented EOT in bash; the mode waits for a bare one.
cat <<-EOT | sed 's/^/> /'
	indented heredoc with $name and $(echo substitution)
	EOT
