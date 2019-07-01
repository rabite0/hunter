#!/bin/sh


errecho() {
    echo ${@} >&2
}

check_dir() {
    DIR=${1}

    [ -d ${DIR} ] ||
	mkdir -p ${DIR} ||
	(echo "Can't create directory: ${DIR}" && exit 1)
}

populate_file() {
    FILE=${1}

    # Don't try to overwrite existing file
    test -e ${FILE} && return



     cat > ${FILE} << EOF
#!/bin/sh

# Selected files are stored here
FILES=\${@}

# You can interate over them one by one
for FILE in \${FILES}; do
    echo \$FILE
done

# Or process them all at once
echo "\${FILES}"
EOF
}


## Starting point

FILE=${1}
MIME=`hunter -m $FILE`
STATUS=$?


# MIME detection failed, bail out unless type is base
[ $STATUS != 0 ] && [ $type != "uni" ] &&
    echo $MIME &&
    exit 1

# Laziy not using XGD here because of OSX
ACTDIR="$HOME/.config/hunter/actions/"

MIME_BASE=`echo $MIME | cut -d "/" -f 1`
MIME_SUB=`echo $MIME | cut -d "/" -f 2`


case $type in
    uni)
	AFILE="${ACTDIR}/${name}.sh"
	check_dir "${ACTDIR}"
	populate_file "${AFILE}"
	$EDITOR "${AFILE}"
	test -e "${AFILE}" && chmod +x "${AFILE}"
	;;
    base)
	BASEDIR="${ACTDIR}/$MIME_BASE"
	AFILE="${BASEDIR}/${name}.sh"
	check_dir "${BASEDIR}"
	populate_file "${AFILE}"
	$EDITOR "${AFILE}"
	test -e ${AFILE} && chmod +x "${ACTDIR}/$name"
	;;
    sub)
	SUBDIR="${ACTDIR}/${MIME_BASE}/${MIME_SUB}"
	AFILE="${SUBDIR}/${name}.sh"
	check_dir ${SUBDIR}
	populate_file "${AFILE}"
	$EDITOR "${AFILE}"
	test -e ${AFILE} && chmod+ +x ${AFILE}
    ;;
esac
